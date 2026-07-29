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

function count(source, needle) {
  return source.split(needle).length - 1;
}

const rust = read('src-tauri/src/windows/focus_widget.rs');
const widget = read('src/pages/FocusWidgetPage.tsx');
const widgetApi = read('src/services/focusWidgetApi.ts');
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
  'one-shot resize events must skip duplicate native window-region rebuilds',
);

const collapseSection = sourceSection(
  rust,
  'fn collapse_window_to_edge(',
  'fn expand_window_to_edge(',
);
const expandSection = sourceSection(
  rust,
  'fn expand_window_to_edge(',
  '#[cfg(windows)]\nfn set_focus_widget_geometry_frame(',
);
for (const [name, section] of [
  ['collapse', collapseSection],
  ['expand', expandSection],
]) {
  assert(
    count(section, 'set_focus_widget_geometry_frame(') === 1,
    `${name} must perform exactly one native geometry update`,
  );
  assert(
    section.includes('prepare_focus_widget_geometry_change(window);') &&
      section.includes('apply_focus_widget_window_shape(window);'),
    `${name} must clear the old region before resizing and apply the final region once`,
  );
}
assert(
  collapseSection.includes('schedule_collapsed_mouse_reentry(app, window, edge, geometry_generation);'),
  'collapsed cursor re-entry must remain available after the one-shot resize',
);
assert(
  expandSection.includes('schedule_peek_mouse_auto_collapse(app);'),
  'expanded pointer-leave fallback must remain available',
);

for (const obsolete of [
  'DOCK_ANIMATION_FRAME_MS',
  'DOCK_EXPAND_RESPONSE_MS',
  'DOCK_COLLAPSE_RESPONSE_MS',
  'FocusWidgetGeometryAnimation',
  'FocusWidgetNativeAnimationContext',
  'animate_focus_widget_geometry',
  'set_focus_widget_animation_frame',
  'run_focus_widget_animation_task_if_current',
  'focus_widget_animation_frame_lock',
  'critically_damped_spring_progress',
  'std::thread::sleep',
  'mpsc::sync_channel',
]) {
  assert(!rust.includes(obsolete), `obsolete per-frame animation path must not return: ${obsolete}`);
}

const suppressionSection = sourceSection(
  rust,
  'fn geometry_event_suppression_duration()',
  'fn note_current_study_mode(',
);
assert(
  suppressionSection.includes('Duration::from_millis(GEOMETRY_SUPPRESSION_GRACE_MS)') &&
    !suppressionSection.includes('GEOMETRY_DEBOUNCE_MS'),
  'geometry suppression must cover only the one-shot resize event burst',
);

const nativeAutoCollapse = sourceSection(
  rust,
  'fn schedule_peek_mouse_auto_collapse(',
  'fn schedule_collapsed_mouse_reentry(',
);
assert(
  nativeAutoCollapse.includes('FOCUS_WIDGET_COLLAPSE_REQUEST_EVENT') &&
    !nativeAutoCollapse.includes('collapse_window_from_current_state'),
  'native pointer polling must request the frontend transition instead of snapping the window closed',
);
assert(
  widgetApi.includes("FOCUS_WIDGET_COLLAPSE_REQUEST_EVENT = 'focus-widget-collapse-requested'"),
  'frontend API must expose the native collapse-request event',
);

const peekSection = sourceSection(
  widget,
  'const peekFromEdge = useCallback',
  'const collapseToEdge = useCallback',
);
const nativeExpandIndex = peekSection.indexOf('await peekFocusWidgetFromEdge()');
const paintIndex = peekSection.indexOf('await waitForAnimationFrame()');
const releaseExpandIndex = peekSection.indexOf('setIsExpanding(false);', paintIndex);
assert(
  nativeExpandIndex >= 0 && paintIndex > nativeExpandIndex && releaseExpandIndex > paintIndex,
  'expansion must resize once, paint the initial CSS state, then release the transition',
);
assert(
  peekSection.includes('if (reversesRetraction) {') &&
    peekSection.indexOf('if (reversesRetraction) {') < nativeExpandIndex,
  'reversing a pending CSS retraction must not issue another native resize',
);

const collapseFrontendSection = sourceSection(
  widget,
  'const collapseToEdge = useCallback',
  'useEffect(() => {\n    if (!canInteract) return undefined;',
);
const transitionIndex = collapseFrontendSection.indexOf('await waitForPanelTransition(panelRef.current)');
const generationCheckIndex = collapseFrontendSection.indexOf(
  'if (transitionGenerationRef.current !== transitionGeneration) return;',
  transitionIndex,
);
const nativeCollapseIndex = collapseFrontendSection.indexOf('await collapseFocusWidgetToEdge()');
assert(
  transitionIndex >= 0 && generationCheckIndex > transitionIndex && nativeCollapseIndex > generationCheckIndex,
  'collapse must finish the interruptible CSS transition and reject stale work before resizing once',
);
assert(
  collapseFrontendSection.includes("dockModeRef.current !== 'peek' || isRetracting"),
  'duplicate native collapse requests must not start concurrent transitions',
);
assert(
  widget.includes('listenTauriEvent(FOCUS_WIDGET_COLLAPSE_REQUEST_EVENT') &&
    widget.includes("event.propertyName === 'transform'") &&
    widget.includes('RETRACT_TRANSITION_FALLBACK_MS'),
  'frontend must handle native collapse requests and use transitionend with a bounded fallback',
);

const frameHelper = sourceSection(
  widget,
  'function waitForAnimationFrame()',
  'function waitForPanelTransition(',
);
assert(
  count(frameHelper, 'window.requestAnimationFrame(') === 1,
  'expansion preparation must wait only one animation frame',
);
assert(!widget.includes('waitForNextPaint'), 'nested two-frame expansion delay must not return');

const motionStyles = sourceSection(
  css,
  '@media (prefers-reduced-motion: no-preference)',
  '.focus-widget-topbar,',
);
assert(
  motionStyles.includes('opacity') && motionStyles.includes('transform'),
  'material transition must retain opacity and transform feedback',
);
assert(!motionStyles.includes('filter'), 'material transition must not animate filter');
assert(!motionStyles.includes('@keyframes'), 'dock transitions must remain interruptible CSS transitions');
assert(
  css.includes('--widget-control-ease: cubic-bezier(0.23, 1, 0.32, 1);') &&
    css.includes('--widget-material-enter: 180ms;') &&
    css.includes('--widget-material-exit: 160ms;'),
  'focus widget motion must use the responsive target curve and sub-300ms durations',
);
assert(!css.includes('blur(32px)'), '32px backdrop blur must not return');
assert(count(css, 'backdrop-filter: blur(16px)') >= 2, 'panel and collapsed tab must use 16px backdrop blur');

const reducedMotion = css.slice(css.lastIndexOf('@media (prefers-reduced-motion: reduce)'));
for (const selector of ['.focus-widget-shell', '.focus-widget-panel', '.focus-widget-collapsed-tab']) {
  assert(reducedMotion.includes(selector), `reduced-motion must cover ${selector}`);
}
assert(
  reducedMotion.includes('animation: none !important') &&
    reducedMotion.includes('transition: none !important') &&
    reducedMotion.includes('transform: none !important'),
  'reduced-motion must disable motion while preserving the state change',
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
  'native re-entry watcher must reject stale work and use OS cursor geometry',
);

const nativePreparation = sourceSection(
  rust,
  'fn prepare_focus_widget_geometry_change(',
  '#[cfg(not(windows))]\nfn prepare_focus_widget_geometry_change(',
);
assert(
  nativePreparation.includes('SetWindowRgn(hwnd, None, false)'),
  'one-shot geometry changes must remove the stale native region before resizing',
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
