# `string-dot-format-extra-positional-arguments` (`F523`)

```toml
[lint]
select = ["F523"]
```

## Regression tests for [#15557]

These should both trigger the rule, but their fixes should not remove the `format` call. In the
first case, `format` will handle escaping `{{` to `{`, while in the second case, removing the
`format` call would suppress a `KeyError`:

```py
print("{{".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
print("{x}".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
```

```snapshot
error[F523]: `.format` call has unused arguments at position(s): 0
 --> src/mdtest_snippet.py:1:7
  |
1 | print("{{".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
  |       ^^^^^^^^^^^^^^^^
help: Remove extra positional arguments at position(s): 0
  |
  - print("{{".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
1 + print("{{".format())  # snapshot: string-dot-format-extra-positional-arguments
2 | print("{x}".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
  |


error[F523]: `.format` call has unused arguments at position(s): 0
 --> src/mdtest_snippet.py:2:7
  |
2 | print("{x}".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
  |       ^^^^^^^^^^^^^^^^^
help: Remove extra positional arguments at position(s): 0
  |
1 | print("{{".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
  - print("{x}".format("!"))  # snapshot: string-dot-format-extra-positional-arguments
2 + print("{x}".format())  # snapshot: string-dot-format-extra-positional-arguments
  |
```

[#15557]: https://github.com/astral-sh/ruff/issues/15557
