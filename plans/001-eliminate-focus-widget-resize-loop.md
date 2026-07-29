# 001 - Eliminate the focus widget resize loop

- **Status**: DONE
- **Commit**: 78520c7
- **Severity**: HIGH
- **Category**: Performance and interruptibility
- **Estimated scope**: 4 files, about 180 lines changed

## Problem

The focus widget expands and collapses by resizing the native WebView window every 16 ms. Each frame is dispatched to the window main thread, waits for a synchronous response, calls `SetWindowPos`, and forces WebView2 to relayout its complete document.

```rust
// src-tauri/src/windows/focus_widget.rs:942 - current
fn animate_focus_widget_geometry(
    window: &WebviewWindow,
    animation: FocusWidgetGeometryAnimation,
) -> Result<bool, String> {
    // ...
    let frame_interval = Duration::from_millis(DOCK_ANIMATION_FRAME_MS);
    // ...
    if !set_focus_widget_animation_frame(
        window,
        generation,
        native_context,
        next_x,
        next_y,
        next_width,
        next_height,
    )? {
        return Ok(false);
    }
}
```

The same relayout interval also repaints two 32 px backdrop filters, multi-layer shadows, and an animated CSS `filter`.

```css
/* src/pages/FocusWidgetPage.css:65 - current */
.focus-widget-panel {
  backdrop-filter: blur(32px) saturate(190%) contrast(105%);
  -webkit-backdrop-filter: blur(32px) saturate(190%) contrast(105%);
}

@media (prefers-reduced-motion: no-preference) {
  .focus-widget-panel {
    transition:
      opacity var(--widget-material-enter) var(--widget-control-ease),
      transform var(--widget-material-enter) var(--widget-control-ease),
      filter 180ms var(--widget-control-ease);
  }
}
```

The frontend starts its material transition before the native window is ready and uses a nested two-frame wait.

```tsx
// src/pages/FocusWidgetPage.tsx:363 - current
if (startsFromCollapsed) {
  await waitForNextPaint();
  if (transitionGenerationRef.current !== transitionGeneration) return;
  setIsExpanding(false);
}

const nextDockState = await peekFocusWidgetFromEdge();
```

## Target

Native geometry must change exactly once per expand or collapse action. No loop, per-frame `SetWindowPos`, synchronous main-thread response channel, spring interpolation, or geometry animation frame lock may remain.

Expansion sequence:

1. React sets `dockState` to `peek` and keeps `isExpanding=true`, rendering the expanded panel in its initial `opacity + transform` state.
2. `peekFocusWidgetFromEdge()` performs one native geometry update to the expanded target and returns.
3. One `requestAnimationFrame` allows the expanded geometry and initial CSS state to paint.
4. React sets `isExpanding=false`; CSS transitions only `opacity` and `transform` for 180 ms using `cubic-bezier(0.23, 1, 0.32, 1)`.

Collapse sequence:

1. React sets `isRetracting=true`; CSS transitions only `opacity` and `transform` for 160 ms using `cubic-bezier(0.23, 1, 0.32, 1)`.
2. Await `transitionend` for the panel's `transform`, with a 220 ms fallback timeout.
3. Re-check `transitionGenerationRef` before invoking the backend so pointer re-entry can cancel the pending collapse.
4. `collapseFocusWidgetToEdge()` performs one native geometry update to the collapsed target, applies the final native region once, emits `collapsed`, and returns.

Material target:

```css
.focus-widget-shell {
  --widget-control-ease: cubic-bezier(0.23, 1, 0.32, 1);
  --widget-material-enter: 180ms;
  --widget-material-exit: 160ms;
}

.focus-widget-panel {
  backdrop-filter: blur(16px) saturate(165%) contrast(103%);
  -webkit-backdrop-filter: blur(16px) saturate(165%) contrast(103%);
}

@media (prefers-reduced-motion: no-preference) {
  .focus-widget-panel {
    transition:
      opacity var(--widget-material-enter) var(--widget-control-ease),
      transform var(--widget-material-enter) var(--widget-control-ease);
  }
}
```

The collapsed tab uses the same 16 px backdrop blur. Do not animate `filter` anywhere in the focus widget transition.

## Repo conventions to follow

- Dock commands and native geometry ownership remain in `src-tauri/src/windows/focus_widget.rs`.
- Frontend transition cancellation continues to use `transitionGenerationRef` and `dockModeRef` from `src/pages/FocusWidgetPage.tsx:88`.
- Existing `prefers-reduced-motion` handling in `src/pages/FocusWidgetPage.css` remains intact.
- Extend `scripts/test-focus-widget-motion.mjs`; do not create a second overlapping source-assertion script.

## Steps

1. In `src-tauri/src/windows/focus_widget.rs`, replace `spawn_focus_widget_geometry_animation` calls in collapse and expand with one direct target geometry update. Apply size constraints before the update, update the dock state only at the correct boundary, apply the native shape once after the update, and preserve collapsed mouse re-entry plus peek auto-collapse scheduling.
2. Remove the now-unused animation machinery: `FocusWidgetGeometryAnimation`, `DockAnimationKind`, native animation context fields, animation duration/frame constants, `mpsc`, the frame lock, spring/lerp helpers, per-frame dispatch helpers, and their obsolete Rust tests. Preserve the monotonic generation counter for stale pointer watcher cancellation if still needed.
3. Set geometry event suppression to cover only the one-shot resize and its event burst: `GEOMETRY_SUPPRESSION_GRACE_MS` (80 ms), not the deleted 340-360 ms animation duration.
4. In `src/pages/FocusWidgetPage.tsx`, replace `waitForNextPaint()` with a one-frame helper. For expansion, await the one-shot native resize first, then one paint, then release `isExpanding`.
5. Add an interruptible `waitForPanelTransition` helper that listens for the panel `transform` transition end and has a 220 ms timeout. In collapse, set `isRetracting`, await it, check the generation, then call the one-shot native collapse. Remove listeners and timeout in all paths.
6. In `src/pages/FocusWidgetPage.css`, apply the exact target easing, durations, 16 px material blur, and opacity/transform-only transitions. Remove focus-widget transition-time `filter` declarations and `filter` from `will-change`.
7. Rewrite `scripts/test-focus-widget-motion.mjs` assertions so they require one-shot native geometry updates, reject `DOCK_ANIMATION_FRAME_MS`, `animate_focus_widget_geometry`, per-frame dispatch, nested rAF, and transition-time filter animation, while preserving interruptibility and reduced-motion assertions.

## Boundaries

- Do NOT change study timer, pause/resume, break, close, pin, or focus-lock behavior.
- Do NOT animate `width`, `height`, `top`, `left`, margins, padding, or CSS variables.
- Do NOT add dependencies.
- Do NOT change widget dimensions, docking thresholds, hover delays, or saved geometry semantics.
- Do NOT alter non-widget CSS.
- If the cited structures have drifted since commit `78520c7`, stop and report instead of improvising.

## Verification

- **Completed on**: 2026-07-29
- **Passed**: `npm run typecheck`, `node scripts/test-focus-widget-motion.mjs`, `npm run build`, `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and `cargo test --manifest-path src-tauri/Cargo.toml`.

- **Mechanical**: `npm run typecheck`, `node scripts/test-focus-widget-motion.mjs`, `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and `cargo test --manifest-path src-tauri/Cargo.toml` all exit 0.
- **Source check**: `src-tauri/src/windows/focus_widget.rs` contains no `DOCK_ANIMATION_FRAME_MS`, `animate_focus_widget_geometry`, `set_focus_widget_animation_frame`, or 16 ms native geometry loop.
- **Feel check**: run the Tauri app and repeatedly enter/leave every dock edge. The native window changes size once per direction; the material transition remains smooth under CPU load; pointer re-entry during the 160 ms retract reverses the CSS transition and never collapses stale state.
- **DevTools check**: Performance recording shows no repeated layout events caused by 16 ms window resize during dock transitions. CSS Animations shows only `transform` and `opacity` for the panel.
- **Reduced motion**: movement remains disabled with `prefers-reduced-motion: reduce`; docking still changes state correctly.
- **Done when**: expansion and collapse perform a single native geometry mutation each, no transition animates paint/layout properties, rapid reversal remains correct, and all mechanical checks pass.
