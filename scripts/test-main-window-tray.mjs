/**
 * 主窗口「关闭即隐藏到托盘」回归校验。
 *
 * 复现过的线上 bug：空闲（没有学习模式、没有专注记录、没有闹钟）时点窗口右上角 X，
 * `CloseRequested` 因为 `should_prevent_exit()` 返回 false 而没有拦截，主窗口被销毁；
 * 但进程还被 `focus-widget` 窗口留在后台，托盘也还在。于是：
 *   1. 再点桌面快捷方式，单实例插件拦下新进程，回调里 `get_webview_window("main")`
 *      拿到 None，什么也没发生 —— 表现就是「应用打不开」；
 *   2. 托盘「显示主界面」同样拿到 None，也打不开；
 *   3. 只能去任务管理器彻底结束进程才能重开。
 *
 * 修复后的契约：
 *   - 主窗口的 `CloseRequested` 无条件 `prevent_close` + `hide`，窗口永不销毁；
 *   - `show_main_window` 在窗口缺失时按 `tauri.conf.json` 重建，且建窗放到后台线程，
 *     避免在托盘/单实例主线程回调里重入事件循环；
 *   - 托盘左键、托盘菜单、单实例唤起、提醒唤醒、悬浮窗「回到主界面」都走这条统一入口；
 *   - 真正退出只保留托盘菜单「退出」（`app.exit`）。
 *
 * 真实窗口行为的人工验收步骤见 FEATURES.md 第 9 节。
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(process.cwd());
const read = (relativePath) => readFileSync(resolve(root, relativePath), 'utf8').replace(/\r\n/g, '\n');

let failed = false;
function assert(condition, message) {
  if (!condition) {
    console.error(`FAIL: ${message}`);
    failed = true;
  }
}

function sourceSection(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert(start >= 0, `missing ${startMarker}`);
  assert(end > start, `missing ${endMarker} after ${startMarker}`);
  return start >= 0 && end > start ? source.slice(start, end) : '';
}

/** 去掉 `//` 行注释，避免注释里的说明文字干扰断言。 */
function stripLineComments(source) {
  return source
    .split('\n')
    .map((line) => line.replace(/\/\/.*$/, ''))
    .join('\n');
}

const lib = read('src-tauri/src/lib.rs');
const focusWidget = read('src-tauri/src/windows/focus_widget.rs');
const tauriConfig = JSON.parse(read('src-tauri/tauri.conf.json'));

// ---------- 1. 主窗口关闭一律收进托盘，不再按运行态决定是否销毁 ----------
const onWindowEvent = sourceSection(lib, '.on_window_event(', '.invoke_handler(');
const mainCloseRequested = sourceSection(
  onWindowEvent,
  'WindowEvent::CloseRequested { api, .. } => {',
  '_ => {}',
);
assert(
  mainCloseRequested.includes('api.prevent_close();'),
  '主窗口关闭必须无条件 prevent_close：空闲时销毁窗口正是「后台运行后打不开」的根因',
);
assert(
  mainCloseRequested.includes('hide_main_window_to_tray(window);'),
  '主窗口关闭必须收敛成隐藏到托盘',
);
assert(
  !mainCloseRequested.includes('should_prevent_exit'),
  '关闭主窗口不能再按运行态分支：空闲分支会把窗口销毁掉',
);
assert(
  !mainCloseRequested.includes('window.hide()'),
  '隐藏动作应走 hide_main_window_to_tray，避免各处重复实现',
);

// 悬浮小组件的关闭处理不能被顺手改坏。
assert(
  onWindowEvent.includes('focus_widget::FOCUS_WIDGET_LABEL'),
  '悬浮小组件的 CloseRequested 分支必须保留',
);

// ---------- 2. 显示入口在窗口缺失时会重建 ----------
const showMainWindow = sourceSection(lib, 'pub(crate) fn show_main_window(', '\n}\n');
assert(
  showMainWindow.includes('match app.get_webview_window(MAIN_WINDOW_LABEL)'),
  'show_main_window 必须区分「窗口还在」和「窗口已被销毁」两种情况',
);
assert(
  showMainWindow.includes('recreate_main_window(app)'),
  '窗口被销毁时必须重建，否则托盘/单实例唤起会静默失败',
);
assert(
  !showMainWindow.includes('Ok(())') || showMainWindow.indexOf('recreate_main_window') < showMainWindow.lastIndexOf('Ok(())'),
  '重建路径必须在返回 Ok 之前接上',
);

// ---------- 3. 重建不能在托盘 / 单实例的主线程回调里同步建窗 ----------
const recreate = stripLineComments(sourceSection(lib, 'fn recreate_main_window(', 'fn create_main_window('));
assert(
  recreate.includes('std::thread::spawn'),
  '重建窗口必须离开主线程：托盘点击与单实例 WM_COPYDATA 回调都在主线程，同步建窗会重入事件循环',
);
assert(
  recreate.includes('main_window_create_lock()'),
  '重建需要互斥：托盘连点与单实例唤起可能同时要求显示',
);
assert(
  recreate.includes('get_webview_window(MAIN_WINDOW_LABEL)'),
  '重建前要复查，避免并发路径重复建出同名窗口',
);
assert(
  recreate.includes('reveal_main_window(&window)'),
  '重建后必须显式显示并聚焦',
);

const createMainWindow = stripLineComments(sourceSection(lib, 'fn create_main_window(', 'fn wide_null('));
assert(
  createMainWindow.includes('WebviewWindowBuilder::from_config(app, &config)'),
  '重建必须复用 tauri.conf.json 的主窗口配置（尺寸、最小尺寸、CSP 等）',
);
assert(
  createMainWindow.includes('window.label == MAIN_WINDOW_LABEL'),
  '重建必须从配置里挑出 label 为 main 的那一项',
);
assert(
  tauriConfig.app.windows.some((window) => window.label === 'main'),
  'tauri.conf.json 必须保留 main 窗口配置，重建才有依据',
);

// ---------- 4. 所有「叫回主界面」的入口都走同一条路 ----------
assert(
  /tauri_plugin_single_instance::init\(\|app, _argv, _cwd\| \{\s*if let Err\(error\) = show_main_window\(app\)/.test(lib),
  '单实例唤起（再次双击快捷方式）必须显示/重建主窗口',
);
const traySection = sourceSection(lib, '.on_menu_event(|app, event|', '.build(app)?');
assert(
  sourceSection(traySection, '"tray_show" =>', '"tray_toggle_focus_widget" =>').includes(
    'show_main_window(app)',
  ),
  '托盘菜单「显示主界面」必须显示/重建主窗口',
);
assert(
  sourceSection(traySection, '"tray_toggle_focus_widget" =>', '"tray_quit" =>').includes(
    'toggle_visibility',
  ),
  '托盘菜单的悬浮窗开关应保持原样',
);
const trayQuit = sourceSection(traySection, '"tray_quit" =>', '_ => {}');
assert(
  trayQuit.includes('should_prevent_exit') && trayQuit.includes('app.exit(0)'),
  '真正退出只保留托盘「退出」：有活动学习模式时阻止并显示主界面，否则 app.exit',
);
assert(
  traySection.includes('show_main_window(tray.app_handle())'),
  '托盘图标左键必须显示/重建主窗口',
);

const wakeReminder = stripLineComments(sourceSection(lib, 'fn wake_main_window_for_reminder(', 'pub(crate) fn sync_focus_widget_window('));
assert(
  wakeReminder.indexOf('show_main_window(app)') >= 0 &&
    wakeReminder.indexOf('show_main_window(app)') < wakeReminder.indexOf('get_webview_window'),
  '提醒唤醒必须先确保主窗口存在，再取窗口做置顶',
);

// ---------- 5. 悬浮窗「回到主界面」不再硬失败 ----------
const bringToMain = stripLineComments(
  sourceSection(focusWidget, 'pub fn bring_to_main(&self)', 'pub fn refresh_geometry(&self)'),
);
assert(
  bringToMain.includes('crate::show_main_window(&self.app)'),
  '悬浮窗回到主界面必须复用统一入口，窗口被销毁时也能回来',
);
assert(
  !bringToMain.includes('Main window not found'),
  '不能因为主窗口缺失就直接报错，用户会看到「点了没反应」',
);

if (failed) {
  console.error('\n主窗口托盘行为校验失败。');
  process.exit(1);
}

console.log('主窗口托盘行为校验通过：');
console.log('  - 关闭主窗口只会隐藏到托盘，窗口不再被销毁');
console.log('  - 托盘 / 单实例 / 提醒 / 悬浮窗都经 show_main_window 显示或重建');
console.log('  - 窗口缺失时在后台线程按 tauri.conf.json 重建，不重入事件循环');
console.log('  - 真正退出仍然只有托盘菜单「退出」');
