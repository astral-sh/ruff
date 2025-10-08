# Hover type assertions

This document tests the hover assertion functionality in mdtest. Hover assertions allow testing
the inferred type at a specific position in the code, similar to how a language server's hover
feature works.

## Basic hover assertions

The `# ↓ hover:` syntax lets us assert the type at a specific column position. The down arrow (↓)
indicates the column where we want to check the type.

```py
def test_basic_types() -> None:
    # Test basic literals
    number = 10
    #   ↓ hover: Literal[10]
    number

    text = "hello"
    #   ↓ hover: Literal["hello"]
    text

    # Test variables with type annotations
    annotated: int = 42
    #   ↓ hover: int
    annotated
```

## Hover on different expression types

### Literals

```py
# Integer literal (standalone expression statement)
#        ↓ hover: Literal[42]
value1 = 42

# String literal
#        ↓ hover: Literal["test"]
value2 = "test"

# Boolean literal
#        ↓ hover: Literal[True]
value3 = True
```

### Names and attributes

```py
class MyClass:
    value: int = 10

def test_attributes() -> None:
    instance = MyClass()

    # Hover on simple name
    #   ↓ hover: MyClass
    instance

    # Hover on attribute access
    #   ↓ hover: int
    instance.value
```

### Function definitions

```py
def get_number() -> int:
    return 42

# Hover on a function name shows its type
#          ↓ hover: def get_number() -> int
result = get_number
```

### Binary operations

```py
def test_binary_ops() -> None:
    first = 10
    second = 20

    # Hover on left operand
    #   ↓ hover: Literal[10]
    first + second

    # Hover on right operand
    #          ↓ hover: Literal[20]
    first + second

    # Hover on the entire binary expression
    #   ↓ hover: Literal[30]
    first + second
```

### Comprehensions

```py
# List comprehension
#        ↓ hover: list[@Todo(list comprehension element type)]
result = [x for x in range(5)]
```

## Edge cases

### Hover near start of expression

```py
# The down arrow must point to a character within the expression
#        ↓ hover: Literal[1]
value = 1 + 2
```

### Hover with unicode characters

```py
def test_unicode() -> None:
    # Variable name with unicode
    café = "coffee"
    #   ↓ hover: Literal["coffee"]
    café

    # String literal with unicode
    #           ↓ hover: Literal["hello 世界"]
    result = "hello 世界"
```

## Multiple hovers on the same line

We can have multiple hover assertions for different positions on the same line:

```py
def multiple_hovers() -> None:
    # Multiple hovers on literals in one expression
    #        ↓ hover: Literal[1]
    #            ↓ hover: Literal[2]
    #                ↓ hover: Literal[3]
    total = 1 + 2 + 3

    # Multiple hovers in function call arguments
    #            ↓ hover: Literal[5]
    #               ↓ hover: Literal[3]
    result = max(5, 3)
```

