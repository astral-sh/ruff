# `superfluous-else-return` (`RET505`)

```toml
lint.select = ["RET505"]
```

## Multiple leading comments

Removing the `else` dedents its body based on the first statement, regardless of the indentation of
leading comments. The second `return` stays outside the `if`, so `f(False)` still returns `2`.

```py
def f(condition):
    if condition:
        return 1
    else:  # snapshot: superfluous-else-return
        # First comment.
          # Second comment.
        return 2
```

```snapshot
error[RET505]: Unnecessary `else` after `return` statement
 --> src/mdtest_snippet.py:4:5
  |
4 |     else:  # snapshot: superfluous-else-return
  |     ^^^^
help: Remove unnecessary `else`
  |
3 |         return 1
  -     else:  # snapshot: superfluous-else-return
  -         # First comment.
  -           # Second comment.
  -         return 2
4 +     # snapshot: superfluous-else-return
5 +     # First comment.
6 +       # Second comment.
7 +     return 2
  |
```
