# TODO System Improvements

## Status Summary

| Feature | Status |
|---------|--------|
| Priority Visualization | Done |
| Keyboard Shortcuts | Removed (unnecessary) |
| TODO Filtering | Not planned |
| TODO Notifications | Not planned |
| TODO Analytics | Not planned |
| Auto-Priority Adjustment | Not planned |

---

## 1. Priority Visualization in TUI

**Status: Done**

Priority visualization is implemented in `crates/uira-tui/src/app.rs` (lines 542-550):

- High priority: Red lightning bolt
- Medium priority: Yellow bullet
- Low priority: Default color

---

## Archived (Not Planned)

The following features were considered but deemed unnecessary for the current use case:

### Keyboard Shortcuts
Toggle TODO sidebar with Ctrl+T, quick mark done with Ctrl+D. Not needed - TUI focus is on agent interaction, not manual TODO management.

### TODO Filtering
Filter by status/priority. Over-engineering for current use case.

### TODO Notifications
System notifications for high-priority completions. Adds complexity without clear benefit.

### TODO Analytics
Completion rate, avg time stats. Not useful for agent-managed TODOs.

### Auto-Priority Adjustment
Escalate priority of old TODOs. Agent already handles prioritization.