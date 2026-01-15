# Add debouncing and exclusion filtering to `--watch` mode (#19573)

## Summary

This PR addresses [#19573](https://github.com/astral-sh/ruff/issues/19573) by improving `ruff --watch` with two key enhancements:

1. **Event debouncing** - Batches rapid file changes to prevent redundant linting
2. **Exclusion pattern filtering** - Respects `exclude` and `extend-exclude` patterns for watch triggers

## Changes

### New Functions

- `is_path_excluded(path, file_resolver)` - Checks if a path matches exclusion patterns by walking up ancestors
- `process_event(event, file_resolver, change)` - Processes watcher events and tracks the highest-priority change kind

### Modified Functions

- `change_detected(event, file_resolver)` - Now accepts `FileResolverSettings` and skips excluded paths

### Watch Loop Improvements

- **Debouncing**: After receiving the first event, collects subsequent events for up to 10ms of idle time or 3 seconds max before triggering a lint run
- **Exclusion filtering**: Events from excluded directories (`.venv`, `node_modules`, `.git`, `__pycache__`, etc.) no longer trigger watch cycles

## Implementation Details

### Debounce Constants

```rust
const DEBOUNCE_IDLE: Duration = Duration::from_millis(10);  // Flush after 10ms idle
const DEBOUNCE_MAX: Duration = Duration::from_secs(3);      // Max 3s debounce window
```

### Exclusion Check

The `is_path_excluded` function checks the path and all its ancestors against both `exclude` and `extend_exclude` patterns from the configuration, reusing the existing `match_exclusion` utility.

## Test Plan

- [x] Existing `detect_correct_file_change` test passes
- [x] New `excluded_paths_are_ignored` test verifies exclusion for `.venv`, `node_modules`, `.git`, `__pycache__`
- [x] Clippy passes
- [x] Pre-commit checks pass

## Before/After

**Before**: Every file change (including in `.venv`, `node_modules`) triggered an immediate lint run, causing unnecessary CPU usage and visual flashing.

**After**:
- Changes in excluded directories are ignored
- Rapid file saves are batched together
- Single lint run after activity settles (10ms idle) or at most every 3 seconds
