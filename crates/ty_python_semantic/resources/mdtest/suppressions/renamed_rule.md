# Renamed rules in ignore comments

The mdtest registry defines `old-rule-name` as an alias for `new-rule-name`. The rule never emits a
diagnostic, so these examples disable warnings about unused ignore comments.

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

When a comment already contains the current name, ty reports the rename without offering a fix.

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
```

If the old name suppresses a diagnostic, `unused-type-ignore-comment` can offer to remove the
current name. Here, `other-tool` separates the two names. A fix that removed the old name would
touch a different part of the comment, so `--fix` could apply both deletions in one pass and expose
the diagnostic.

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
```

## Suppressing the rule

```py
x = 1  # ty: ignore[old-rule-name, renamed-rule-in-ignore-comment]
```
