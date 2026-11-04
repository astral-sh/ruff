# Renamed rules in ignore comments

```toml
[rules]
renamed-rule-in-ignore-comment = "warn"
```

## Line-level ignores

The old name still suppresses the import error. The fix replaces just the rule name, preserving the
`ty:` prefix and other type checkers' codes in a `type: ignore` comment.

```py
# snapshot
import another_module  # type: ignore[unresolved-import, ty:unresolved-import]  # optional
```

```snapshot
warning[renamed-rule-in-ignore-comment]: Rule `unresolved-import` has been renamed to `import-not-found`
 --> src/mdtest_snippet.py:2:61
  |
2 | import another_module  # type: ignore[unresolved-import, ty:unresolved-import]  # optional
  |                                                             ^^^^^^^^^^^^^^^^^
help: Replace `unresolved-import` with `import-not-found`
  |
1 | # snapshot
  - import another_module  # type: ignore[unresolved-import, ty:unresolved-import]  # optional
2 + import another_module  # type: ignore[unresolved-import, ty:import-not-found]  # optional
  |
```

## File-level ignores

```py
# error: [renamed-rule-in-ignore-comment]
# ty: ignore[unresolved-import]

import missing_module
```

## Both names in one comment

When a comment already contains the current name, ty reports the rename without offering a fix.

```toml
[rules]
unused-ignore-comment = "ignore"
unused-type-ignore-comment = "ignore"
```

```py
# snapshot
import missing_module  # ty: ignore[unresolved-import, import-not-found]
```

```snapshot
warning[renamed-rule-in-ignore-comment]: Rule `unresolved-import` has been renamed to `import-not-found`
 --> src/mdtest_snippet.py:2:37
  |
2 | import missing_module  # ty: ignore[unresolved-import, import-not-found]
  |                                     ^^^^^^^^^^^^^^^^^
```

```py
# snapshot
import another_module  # type: ignore[ty:import-not-found, ty:unresolved-import]
```

```snapshot
warning[renamed-rule-in-ignore-comment]: Rule `unresolved-import` has been renamed to `import-not-found`
 --> src/mdtest_snippet.py:4:63
  |
4 | import another_module  # type: ignore[ty:import-not-found, ty:unresolved-import]
  |                                                               ^^^^^^^^^^^^^^^^^
```

## Suppressing the rule

```py
import missing_module  # ty: ignore[unresolved-import, renamed-rule-in-ignore-comment]
```
