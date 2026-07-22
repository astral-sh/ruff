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
        reveal_type(x)  # revealed: Literal[1]

def _(flag: bool):
    while flag and (x := returns_bool()):
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

## Nested short-circuit assignments

Assignments in mutually exclusive short-circuit paths can still leave a name definitely bound.

```py
def _(flag: bool):
    if (flag and (x := 54)) or (x := 32):
        reveal_type(x)  # revealed: Literal[54, 32]

def _(flag: bool):
    (flag and (x := 1)) or (x := 2)
    reveal_type(x)  # revealed: Literal[1, 2]

def _(flag: bool, possibly_falsy_int: int, possibly_falsy_str: str):
    (flag and (x := possibly_falsy_int)) or (x := possibly_falsy_str)
    reveal_type(x)  # revealed: int | str

def _(flag: bool):
    (flag or (x := 0)) and (x := 2)
    reveal_type(x)  # revealed: Literal[0, 2]

def _(flag1: bool, flag2: bool):
    if (flag1 and (x := 1)) or (flag2 and (x := 2)):
        reveal_type(x)  # revealed: Literal[1, 2]

    if (flag1 or (y := 0)) and (flag2 or (y := 0)):
        pass
    else:
        reveal_type(y)  # revealed: Literal[0]

def _(flag1: bool, flag2: bool):
    (flag1 and (x := 1)) or (flag2 and (x := 2)) or (x := 3)
    reveal_type(x)  # revealed: Literal[1, 2, 3]

def _(flag1: bool):
    if (flag1 and (y := 1)) or (z := 2):
        # error: [possibly-unresolved-reference]
        reveal_type(y)  # revealed: Literal[1]
        # error: [possibly-unresolved-reference]
        reveal_type(z)  # revealed: Literal[2]
```

## Negated expressions

```py
def _(x: str):
    if not (x and (y := x)):
        raise ValueError

    reveal_type(y)  # revealed: str
```

## Other condition consumers

```py
def assert_statement(flag: bool):
    assert flag and (x := 1)
    reveal_type(x)  # revealed: Literal[1]

def if_expression(flag: bool):
    reveal_type(x) if flag and (x := 1) else None  # revealed: Literal[1]

def match_guard(flag: bool, subject: object):
    match subject:
        case _ if flag and (x := 1):
            reveal_type(x)  # revealed: Literal[1]

def comprehension_filter(flag: bool):
    [reveal_type(x) for _ in range(1) if flag and (x := 1)]  # revealed: Literal[1]
```
