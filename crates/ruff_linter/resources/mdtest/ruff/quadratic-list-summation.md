# `quadratic-list-summation` (`RUF017`)

```toml
target-version = "py314"
lint.select = ["RUF017"]
```

On Python 3.14, the fix uses `functools.reduce` because unpacking comprehensions require Python 3.15.

## Missing imports

```py
lists = [[1, 2], [3, 4]]
sum(lists, [])  # snapshot: quadratic-list-summation
```

```snapshot
error[RUF017]: Avoid quadratic list summation
 --> src/mdtest_snippet.py:2:1
  |
2 | sum(lists, [])  # snapshot: quadratic-list-summation
  | ^^^^^^^^^^^^^^
help: Replace with `functools.reduce`
  |
1 + import functools
2 + import operator
3 | lists = [[1, 2], [3, 4]]
  - sum(lists, [])  # snapshot: quadratic-list-summation
4 + functools.reduce(operator.iadd, lists, [])  # snapshot: quadratic-list-summation
  |
note: This is an unsafe fix and may change runtime behavior
```

## Existing combined import

An existing combined import is reused without generating duplicate edits.

```py
import functools, operator

sum([[1, 2], [3, 4]], [])  # snapshot: quadratic-list-summation
```

```snapshot
error[RUF017]: Avoid quadratic list summation
 --> src/mdtest_snippet.py:3:1
  |
3 | sum([[1, 2], [3, 4]], [])  # snapshot: quadratic-list-summation
  | ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `functools.reduce`
  |
2 |
  - sum([[1, 2], [3, 4]], [])  # snapshot: quadratic-list-summation
3 + functools.reduce(operator.iadd, [[1, 2], [3, 4]], [])  # snapshot: quadratic-list-summation
  |
note: This is an unsafe fix and may change runtime behavior
```
