# Renamed rules in ignore comments

For purposes of testing the `renamed-rule-in-ignore-comment` lint rule, the mdtest registry defines
`old-rule-name` as an alias for `new-rule-name`. These dummy rules do not actually emit a
diagnostic, so we ignore unused ignore comments in this test suite.

```toml
[rules]
renamed-rule-in-ignore-comment = "warn"
unused-ignore-comment = "ignore"
unused-type-ignore-comment = "ignore"
```

## Line-level ignores

The fix replaces just the ty rule name, preserving its `ty:` prefix and other type checkers' codes.

```py
# snapshot
x = 1  # type: ignore[old-rule-name, ty:old-rule-name]  # keep this comment
```

```snapshot
warning[renamed-rule-in-ignore-comment]: Rule `old-rule-name` has been renamed to `new-rule-name`
 --> src/mdtest_snippet.py:2:41
  |
2 | x = 1  # type: ignore[old-rule-name, ty:old-rule-name]  # keep this comment
  |                                         ^^^^^^^^^^^^^
help: Replace `old-rule-name` with `new-rule-name`
  |
1 | # snapshot
  - x = 1  # type: ignore[old-rule-name, ty:old-rule-name]  # keep this comment
2 + x = 1  # type: ignore[old-rule-name, ty:new-rule-name]  # keep this comment
  |
```

## File-level ignores

```py
# error: [renamed-rule-in-ignore-comment]
# ty: ignore[old-rule-name]

x = 1
```

## Both names in one comment

The fix renames the old name even when the comment already contains the new name. The unused-ignore
rules can then remove the redundant entry.

In an inline `ty: ignore[new-rule-name, other-rule, old-rule-name]`, the old name suppresses the
diagnostic, so `unused-ignore-comment` can remove `new-rule-name`. If the rename fix also removed
`old-rule-name`, the two deletions wouldn't overlap and `--fix` could apply both in one pass. That
would leave `ty: ignore[other-rule]` and expose the diagnostic.

```py
# snapshot
x = 1  # ty: ignore[old-rule-name, new-rule-name]
```

```snapshot
warning[renamed-rule-in-ignore-comment]: Rule `old-rule-name` has been renamed to `new-rule-name`
 --> src/mdtest_snippet.py:2:21
  |
2 | x = 1  # ty: ignore[old-rule-name, new-rule-name]
  |                     ^^^^^^^^^^^^^
help: Replace `old-rule-name` with `new-rule-name`
  |
1 | # snapshot
  - x = 1  # ty: ignore[old-rule-name, new-rule-name]
2 + x = 1  # ty: ignore[new-rule-name, new-rule-name]
3 | # snapshot
  |
```

```py
# snapshot
y = 2  # type: ignore[ty:new-rule-name, other-tool, ty:old-rule-name]
```

```snapshot
warning[renamed-rule-in-ignore-comment]: Rule `old-rule-name` has been renamed to `new-rule-name`
 --> src/mdtest_snippet.py:4:56
  |
4 | y = 2  # type: ignore[ty:new-rule-name, other-tool, ty:old-rule-name]
  |                                                        ^^^^^^^^^^^^^
help: Replace `old-rule-name` with `new-rule-name`
  |
3 | # snapshot
  - y = 2  # type: ignore[ty:new-rule-name, other-tool, ty:old-rule-name]
4 + y = 2  # type: ignore[ty:new-rule-name, other-tool, ty:new-rule-name]
  |
```

## Suppressing the rule

```py
x = 1  # ty: ignore[old-rule-name, renamed-rule-in-ignore-comment]
```
