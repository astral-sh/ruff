# Diagnostics for bad subscripts on dicts

## Simple case

```py
def f(x: dict[str, str]):
    y = x[42]  # snapshot: invalid-argument-type
    reveal_type(y)  # revealed: str

    # error: [invalid-argument-type] "Cannot subscript an object of type `dict[str, str]` with a key of type `Literal[56]` (expected `str`)"
    x[56]
```

```snapshot
error[invalid-argument-type]: Invalid subscript read
 --> src/mdtest_snippet.py:2:9
  |
2 |     y = x[42]  # snapshot: invalid-argument-type
  |         -^^^^
  |         | |
  |         | Expected `str`, got object of type `Literal[42]`
  |         Has type `dict[str, str]`
  |
info: This subscript expression implicitly calls `dict[str, str].__getitem__`
    --> stdlib/builtins.pyi:3170:9
     |
3170 |     def __getitem__(self, key: _KT, /) -> _VT:
     |         ^^^^^^^^^^^ Method defined here
     |
```
