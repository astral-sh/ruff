# Nested iterable types

Regression test for <https://github.com/astral-sh/ty/issues/3827>.

```py
while 1:
    x = iter([[None] + [x]])  # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Iterator[Divergent]
```
