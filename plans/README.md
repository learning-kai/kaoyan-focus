# Animation plans

| Number | Title | Severity | Status |
| --- | --- | --- | --- |
| 001 | Eliminate the focus widget resize loop | HIGH | DONE |

## Recommended order

1. Execute `001-eliminate-focus-widget-resize-loop.md` as one atomic change. Its Rust one-shot geometry update and frontend transition timing depend on each other and must not ship separately.
