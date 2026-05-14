# Short-Circuit Evaluation

## Not all boolean expressions must be evaluated

In `or` expressions, if the left-hand side is truthy, the right-hand side is not evaluated.
Similarly, in `and` expressions, if the left-hand side is falsy, the right-hand side is not
evaluated.

```py
def _(flag1: bool, flag2: bool):
    if flag1:
        pass
    elif flag2 or (x := 1):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool):
    if flag1 or (x := 1):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool, flag2: bool):
    if flag1:
        pass
    elif flag2 and (x := 1):
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool):
    if flag1 and (x := 1):
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool, flag2: bool):
    if flag1 and flag2 and (multi := 1):
        reveal_type(multi)  # revealed: Literal[1]

    if flag1 or (else_or := flag2):
        pass
    else:
        reveal_type(else_or)  # revealed: Literal[False]
```

## TODO: while loops

We currently use the precise truthy and falsy snapshots from boolean operators in `if` statements,
but not yet in `while` loops. These diagnostics should be removed when the same logic is applied to
`while` loop bodies and exits.

```py
def returns_bool() -> bool:
    return False

def _(flag: bool):
    while flag and (x := 1):
        # TODO: should not emit [possibly-unresolved-reference]
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[1]

def _(flag: bool):
    while flag and (x := returns_bool()):
        # TODO: should not emit [possibly-unresolved-reference]
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[True]

def _(flag: bool):
    while flag or (x := returns_bool()):
        pass
    # TODO: should not emit [possibly-unresolved-reference]
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Literal[False]

def _(flag: bool):
    while flag or (x := returns_bool()):
        pass
    else:
        # TODO: should not emit [possibly-unresolved-reference]
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[False]
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

## Negated expressions

```py
def _(x: str):
    if not (x and (y := x)):
        raise ValueError

    reveal_type(y)  # revealed: str & ~AlwaysFalsy
```
