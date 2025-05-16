# Short-Circuit Evaluation

## Not all boolean expressions must be evaluated

In `or` expressions, if the left-hand side is truthy, the right-hand side is not evaluated.
Similarly, in `and` expressions, if the left-hand side is falsy, the right-hand side is not
evaluated.

```py
def _(flag: bool, number: int):
    flag or (y := number)
    # error: [possibly-unresolved-reference]
    reveal_type(y)  # revealed: int

    flag and (x := number)
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: int
```

## First expression is always evaluated

```py
def _(flag: bool):
    if (x := 1) or flag:
        reveal_type(x)  # revealed: Literal[1]

    if (x := 1) and flag:
        reveal_type(x)  # revealed: Literal[1]
```

## Statically known truthiness

```py
if True or (x := 1):
    # error: [unresolved-reference]
    reveal_type(x)  # revealed: Unknown

if True and (x := 1):
    reveal_type(x)  # revealed: Literal[1]
```

## Later expressions can always use variables from earlier expressions

```py
def _(flag: bool):
    flag or (x := 1) or reveal_type(x)  # revealed: Never

    # error: [unresolved-reference]
    flag or reveal_type(y) or (y := 1)  # revealed: Unknown
```

## Nested expressions

```py
def _(flag1: bool, flag2: bool):
    if flag1 or ((x := 1) and flag2):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[1]

    if ((y := 1) and flag1) or flag2:
        reveal_type(y)  # revealed: Literal[1]

    # error: [possibly-unresolved-reference]
    if (flag1 and (z := 1)) or reveal_type(z):  # revealed: Literal[1]
        # error: [possibly-unresolved-reference]
        reveal_type(z)  # revealed: Literal[1]
```

## Inside if-else blocks, we can sometimes know that short-circuit couldn't happen

When if-test contains `And` condition, in the scope of if-body we can be sure that the test is
truthy and therefore short-circuiting couldn't happen. Similarly, when if-test contains `Or`
condition, in the scope of if-else we can be sure that the test is falsy, and therefore
short-circuiting couldn't happen.

### And

```py
def _(flag: bool, number: int):
    if flag and (x := number):
        # x must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysFalsy
    else:
        # TODO: could be int & AlwaysFalsy
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int

    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: int
```

### Or

```py
def _(flag: bool, number: int):
    if flag or (x := number):
        # TODO: could be int & AlwaysTruthy
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int
    else:
        # x must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysTruthy

    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: int
```

### Elif

```py
def _(flag: bool, flag2: bool, number: int):
    if flag or (x := number):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int
    elif flag2 or (y := number):
        # x must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysTruthy

        # error: [possibly-unresolved-reference]
        reveal_type(y)  # revealed: int
    else:
        # x and y must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysTruthy
        reveal_type(y)  # revealed: int & ~AlwaysTruthy

    if flag or (x := number):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int
    elif flag2 and (y := number):
        # x must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysTruthy

        reveal_type(y)  # revealed: int & ~AlwaysFalsy
    else:
        # x must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysTruthy

        # error: [possibly-unresolved-reference]
        reveal_type(y)  # revealed: int

    if flag and (x := number):
        reveal_type(x)  # revealed: int & ~AlwaysFalsy
    elif flag2 or (y := number):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int

        # error: [possibly-unresolved-reference]
        reveal_type(y)  # revealed: int
    else:
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int

        reveal_type(y)  # revealed: int & ~AlwaysTruthy
```

### Nested boolean expression

```py
def _(flag: bool, number: int):
    # error: [possibly-unresolved-reference]
    (flag or (x := number)) and reveal_type(x)  # revealed: int

def _(flag: bool, number: int):
    # x must be defined here
    (flag or (x := number)) or reveal_type(x)  # revealed: int & ~AlwaysTruthy

def _(flag: bool, flag_2: bool, number: int):
    if flag and (flag_2 and (x := number)):
        # x must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysFalsy

def _(flag: bool, flag_2: bool, number: int):
    if flag and (flag_2 or (x := number)):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int
    else:
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int

def _(flag: bool, flag_2: bool, number: int):
    if flag or (flag_2 or (x := number)):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int
    else:
        # x must be defined here
        reveal_type(x)  # revealed: int & ~AlwaysTruthy
```

## This logic can be applied in additional cases that aren't supported yet

### If Expression

```py
def _(flag: bool, number: int):
    # TODO: x must be defined here
    # error: [possibly-unresolved-reference]
    reveal_type(x) if flag and (x := number) else None  # revealed: int & ~AlwaysFalsy
```

### While Statement

```py
def _(flag: bool, number: int):
    while flag and (x := number):
        # TODO: x must be defined here
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int & ~AlwaysFalsy

    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: int

def _(flag: bool, number: int):
    while flag or (x := number):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: int

    # TODO: x must be defined here
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: int & ~AlwaysTruthy
```
