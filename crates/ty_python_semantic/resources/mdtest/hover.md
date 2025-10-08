# Hover type assertions

This document tests the hover assertion functionality in mdtest. Hover assertions allow testing
the inferred type at a specific position in the code, similar to how a language server's hover
feature works.

## Basic hover assertions

The `# ↓ hover:` syntax lets us assert the type at a specific column position. The down arrow (↓)
indicates the column where we want to check the type.

```py
def test_basic_types(parameter: int) -> None:
    # ↓ hover: int
    parameter

    #        ↓ hover: Literal[10]
    number = 10

    #      ↓ hover: Literal["hello"]
    text = "hello"
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

## Edge cases

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

### Hovering works on every character in expression

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
