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

function dependencyNames(manifest, section) {
  return Object.keys(manifest[section] ?? {}).sort();
}

const rust = read('src-tauri/src/windows/focus_widget.rs');
const widget = read('src/pages/FocusWidgetPage.tsx');
const css = read('src/pages/FocusWidgetPage.css');
const manifest = JSON.parse(read('package.json'));
const lockfile = JSON.parse(read('package-lock.json'));
const lockedManifest = lockfile.packages?.[''] ?? {};

const windowHandlers = sourceSection(
  rust,
  'fn attach_window_handlers(',
  'fn apply_study_mode_visibility(',
);
const movedHandler = sourceSection(
  windowHandlers,
  'WindowEvent::Moved(_) => {',
  'WindowEvent::Resized(_)',
);
const resizedHandler = sourceSection(
  windowHandlers,
  'WindowEvent::Resized(_)',
  'WindowEvent::CloseRequested',
);
assert(
  !movedHandler.includes('apply_focus_widget_window_shape'),
  'moving the widget must not rebuild an unchanged native window region',
);
assert(
  resizedHandler.includes('if !should_suppress_geometry_events()') &&
    resizedHandler.includes('apply_focus_widget_window_shape(&window_for_events);'),
  'animation-driven resize events must skip native window-region rebuilds',
);

const collapseSection = sourceSection(
  rust,
  'fn collapse_window_to_edge(',
  'fn expand_window_to_edge(',
);
const expandSection = sourceSection(
  rust,
  'fn expand_window_to_edge(',
  'fn spawn_focus_widget_geometry_animation(',
);
assert(
  !collapseSection.includes('set_focus_widget_geometry_frame('),
  'cancelled collapse must not snap the window to its stale target',
);
for (const [name, section] of [
  ['collapse', collapseSection],
  ['expand', expandSection],
]) {
  const claimIndex = section.indexOf(
    'let (animation_generation, current_geometry) = begin_focus_widget_animation(window)?;',
  );
  const spawnIndex = section.indexOf('spawn_focus_widget_geometry_animation(');
  assert(
    claimIndex >= 0 && spawnIndex > claimIndex,
    `${name} must cancel stale work and capture live geometry before queueing animation`,
  );
}
const beginSection = sourceSection(
  rust,
  '#[cfg(windows)]\nfn begin_focus_widget_animation(',
  '#[cfg(not(windows))]\nfn begin_focus_widget_animation(',
);
assert(
  beginSection.includes('logical_geometry_from_hwnd(hwnd, scale_factor)?') &&
    !beginSection.includes('logical_geometry_from_window(window)?'),
  'Windows cancellation must capture live geometry under the frame lock without Tauri re-entry',
);
assert(
  collapseSection.includes('spawn_focus_widget_geometry_animation(') &&
    !collapseSection.includes('animate_focus_widget_geometry(window'),
  'collapse animation must release the synchronous Tauri command path',
);
assert(
  collapseSection.includes(
    'schedule_collapsed_mouse_reentry(app, window, edge, animation_generation);',
  ),
  'collapse must keep native cursor re-entry interruptible while WebView resize events are busy',
);
const spawnSection = sourceSection(
  rust,
  'fn spawn_focus_widget_geometry_animation(',
  'fn animate_focus_widget_geometry(',
);
assert(
  spawnSection.includes('tauri::async_runtime::spawn_blocking') &&
    spawnSection.includes('Ok(false) => {}') &&
    !spawnSection.includes('begin_focus_widget_animation(window)?'),
  'geometry animation must run off the command path and stale work must exit silently',
);
assert(
  spawnSection.includes('animate_focus_widget_geometry(&window, animation)'),
  'queued work must use the generation and live geometry captured by the command path',
);
assert(
  spawnSection.includes('run_focus_widget_animation_task_if_current(') &&
    spawnSection.includes('apply_focus_widget_native_shape(native_shape);'),
  'only the current animation generation may restore the final native window region',
);
assert(
  rust.includes('struct FocusWidgetGeometryAnimation') &&
    rust.includes('generation: animation_generation') &&
    rust.includes('from: current_geometry'),
  'queued animation must retain the generation and live geometry captured by the command path',
);
assert(
  rust.includes('apply_animation_frame_if_current(generation, current_generation'),
  'every native frame must reject stale animation generations',
);
const frameDispatchSection = sourceSection(
  rust,
  'fn run_focus_widget_animation_task_if_current',
  'fn apply_animation_frame_if_current',
);
assert(
  frameDispatchSection.includes('.run_on_main_thread(move ||') &&
    frameDispatchSection.includes('focus_widget_animation_frame_lock()') &&
    frameDispatchSection.includes('current_window_animation_generation()') &&
    frameDispatchSection.includes('F: FnOnce() -> Result<(), String>'),
  'generation check and native geometry write must execute together on the window main thread',
);
const frameSubmitSection = sourceSection(
  rust,
  'fn set_focus_widget_animation_frame(',
  'fn run_focus_widget_animation_task_if_current',
);
const windowsFrameSubmitSection = sourceSection(
  rust,
  '#[cfg(windows)]\nfn set_focus_widget_animation_frame(',
  '#[cfg(not(windows))]\nfn set_focus_widget_animation_frame(',
);
assert(
  frameSubmitSection.includes('run_focus_widget_animation_task_if_current(') &&
    frameSubmitSection.includes('set_focus_widget_geometry_frame_physical(') &&
    !frameSubmitSection.includes('focus_widget_animation_frame_lock()') &&
    !frameSubmitSection.includes('move |window|'),
  'main-thread frame tasks must use precomputed native geometry without re-entering Tauri dispatch',
);
assert(
  !windowsFrameSubmitSection.includes('window.hwnd()') &&
    !windowsFrameSubmitSection.includes('window.scale_factor()') &&
    windowsFrameSubmitSection.includes('native_context'),
  'each Windows frame must consume the immutable native context captured before animation',
);
assert(
  rust.includes('native_context: FocusWidgetNativeAnimationContext') &&
    rust.includes('native_context: Some(FocusWidgetNativeAnimationContext') &&
    rust.includes('native_context,\n        },'),
  'animation generation must carry the native context captured at its start',
);

for (const name of ['DOCK_EXPAND_RESPONSE_MS', 'DOCK_COLLAPSE_RESPONSE_MS']) {
  const match = rust.match(new RegExp(`const ${name}: u64 = (\\d+);`));
  const responseMs = Number(match?.[1]);
  assert(responseMs >= 320 && responseMs <= 400, `${name} must stay within 320-400ms`);
}
assert(
  rust.includes('fn critically_damped_spring_progress(') &&
    rust.includes('std::f64::consts::TAU') &&
    rust.includes('(-scaled_time).exp()') &&
    rust.includes('.clamp(0.0, 1.0)'),
  'native motion must use a clamped critically damped spring helper',
);
assert(!rust.includes('dock_animation_steps('), 'fixed animation steps must not return');
assert(!rust.includes('cubic_bezier_y_for_x('), 'native cubic-bezier interpolation must not return');
assert(
  rust.includes('geometry_event_suppression_duration()') &&
    rust.includes('DOCK_EXPAND_RESPONSE_MS.max(DOCK_COLLAPSE_RESPONSE_MS)'),
  'geometry event suppression must cover the full spring response',
);
assert(
  rust.includes('critically_damped_spring_is_monotonic_and_never_overshoots') &&
    rust.includes('stale_animation_generation_does_not_apply_its_frame'),
  'spring and stale-generation Rust tests are required',
);

assert(widget.includes('transitionGenerationRef'), 'frontend transition generation guard is missing');
assert(widget.includes('peekFromEdge(true)'), 'pointer re-entry must reverse an active retraction');
assert(
  widget.includes("if (nextDockState.mode === 'peek')") &&
    widget.includes('setIsRetracting(false);'),
  'native re-entry must release the frontend material retraction state',
);
assert(!widget.includes('RETRACT_PREPARE_MS'), 'artificial retract preparation delay must not return');
assert(!widget.includes('waitForMilliseconds'), 'timer-based retract staging must not return');
assert(!css.includes('42ms') && !css.includes('56ms'), 'hard-cut inner-content timings must not return');

const motionStyles = sourceSection(
  css,
  '@media (prefers-reduced-motion: no-preference)',
  '@keyframes focus-widget-tab-in',
);
assert(!motionStyles.includes('translate3d('), 'material animation must not fight native window travel');
assert(motionStyles.includes('opacity') && motionStyles.includes('filter: blur('), 'material transition must animate opacity and blur');
assert(motionStyles.includes('scale(var(--widget-panel-material-scale))'), 'material transition must use only a light scale');

const reducedMotion = css.slice(css.lastIndexOf('@media (prefers-reduced-motion: reduce)'));
for (const selector of ['.focus-widget-shell', '.focus-widget-panel', '.focus-widget-collapsed-tab']) {
  assert(reducedMotion.includes(selector), `reduced-motion must cover ${selector}`);
}
assert(
  reducedMotion.includes('animation: none !important') &&
    reducedMotion.includes('transition: none !important') &&
    reducedMotion.includes('transform: none !important'),
  'reduced-motion must disable animation, transition, and geometric transforms',
);

const nativeReentry = sourceSection(
  rust,
  'fn schedule_collapsed_mouse_reentry(',
  'fn focus_widget_cursor_is_inside_window(',
);
assert(
  nativeReentry.includes('runtime.window_animation_generation == animation_generation') &&
    nativeReentry.includes('observed_outside = !inside') &&
    nativeReentry.includes('focus_widget_native_cursor_is_inside') &&
    nativeReentry.includes('expand_window_to_edge('),
  'native re-entry watcher must validate generation and use OS cursor geometry after outside-to-inside motion',
);
const nativeCursorProbe = sourceSection(
  rust,
  'fn focus_widget_native_cursor_is_inside(',
  'fn focus_widget_cursor_is_inside_window(',
);
assert(
  nativeCursorProbe.includes('GetCursorPos(') && nativeCursorProbe.includes('GetWindowRect('),
  'collapse re-entry probing must not queue behind Tauri/WebView resize events',
);

const nativeAnimationPreparation = sourceSection(
  rust,
  'fn prepare_focus_widget_hwnd_animation(',
  'unsafe extern "system" fn focus_widget_wndproc(',
);
assert(
  nativeAnimationPreparation.includes('SetWindowRgn(hwnd, None, false)') &&
    !nativeAnimationPreparation.includes('DockAnimationKind::Expand'),
  'expand and collapse must both remove the complex native region before their first frame',
);

assert(
  JSON.stringify(dependencyNames(manifest, 'dependencies')) ===
    JSON.stringify(dependencyNames(lockedManifest, 'dependencies')),
  'package.json runtime dependencies changed without a matching lockfile baseline',
);
assert(
  JSON.stringify(dependencyNames(manifest, 'devDependencies')) ===
    JSON.stringify(dependencyNames(lockedManifest, 'devDependencies')),
  'package.json devDependencies changed without a matching lockfile baseline',
);

if (failed) {
  process.exit(1);
}

console.log('Focus widget motion assertions passed');
