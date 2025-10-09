# Hover type assertions

You can use the `hover` assertion to test the inferred type of an expression. This exercises the
same logic as the hover LSP action.

Typically, you will not need to use the `hover` action to test the behavior of our type inference
code, since you can also use `reveal_type` to display the inferred type of an expression. Since
`reveal_type` is part of the standard library, we prefer to use it when possible.

However, there are certain situations where `reveal_type` and `hover` will give different results.
In particular, `reveal_type` is not transparent to bidirectional type checking, as seen in the
"Different results" section below.

## Syntax

### Basic syntax

The `hover` assertion operates on a specific location in the source text. We find the "inner-most"
expression at that position, and then query the inferred type of that expression. The row to query
is identified just like any other mdtest assertion. The column to query is identified by a down
arrow (↓) in the assertion. (Note that the down arrow should always appear immediately before the
`hover` keyword in the assertion.)

```py
def test_basic_types(parameter: int) -> None:
    # ↓ hover: int
    parameter

    #        ↓ hover: Literal[10]
    number = 10

    #      ↓ hover: Literal["hello"]
    text = "hello"
```

### Multiple hovers on the same line

We can have multiple hover assertions for different positions on the same line:

```py
#       ↓ hover: Literal[1]
#           ↓ hover: Literal[2]
#               ↓ hover: Literal[3]
total = 1 + 2 + 3

#            ↓ hover: Literal[5]
#               ↓ hover: Literal[3]
result = max(5, 3)
```

### Hovering works on every character in an expression

```py
def _(param: bool) -> None:
    #        ↓ hover: bool
    #         ↓ hover: bool
    #          ↓ hover: bool
    #           ↓ hover: bool
    #            ↓ hover: bool
    result = param
```

### Hovering with unicode characters

```py
def _(café: str) -> None:
    #        ↓ hover: str
    #         ↓ hover: str
    #          ↓ hover: str
    #           ↓ hover: str
    result = café
```

## Different results for `reveal_type` and `hover`

```py
from typing import overload

def f(x: dict[str, int]) -> None: ...

# revealed: dict[Unknown, Unknown]
f(reveal_type({}))

# ↓ hover: dict[str, int]
f({})
```

## Hovering on different expression types

### Literals

```py
#           ↓ hover: Literal[42]
int_value = 42

#              ↓ hover: Literal["test"]
string_value = "test"

#            ↓ hover: Literal[True]
bool_value = True
```

### Names and attributes

```py
class MyClass:
    value: int

def test_attributes(instance: MyClass) -> None:
    # ↓ hover: MyClass
    instance

    #        ↓ hover: int
    instance.value
```

### Function definitions

```py
def f(x: int) -> None: ...

#        ↓ hover: def f(x: int) -> None
result = f
```

### Binary operations

```py
#        ↓ hover: Literal[10]
#             ↓ hover: Literal[20]
result = 10 + 20
```

### Comprehensions

```py
# List comprehension
#        ↓ hover: list[@Todo(list comprehension element type)]
result = [x for x in range(5)]
```
