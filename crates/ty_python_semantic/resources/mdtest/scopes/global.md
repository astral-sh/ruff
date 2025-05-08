# `global` references

## Implicit global in function

A name reference to a never-defined symbol in a function is implicitly a global lookup.

```py
x = 1

def f():
    reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Explicit global in function

```py
x = 1

def f():
    global x
    reveal_type(x)  # revealed: Unknown | Literal[1]
```

## Unassignable type in function

```py
x: int = 1

def f():
    y: int = 1
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    y = ""

    global x
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    x = ""

    global z
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    z = ""

z: int
```

## Nested intervening scope

A `global` statement causes lookup to skip any bindings in intervening scopes:

```py
x: int = 1

def outer():
    x: str = ""

    def inner():
        global x
        reveal_type(x)  # revealed: int
```

## Narrowing

An assignment following a `global` statement should narrow the type in the local scope after the
assignment.

```py
x: int | None

def f():
    global x
    x = 1
    reveal_type(x)  # revealed: Literal[1]
```

## `nonlocal` and `global`

A binding cannot be both `nonlocal` and `global`. This should emit a semantic syntax error. CPython
marks the `nonlocal` line, while `mypy`, `pyright`, and `ruff` (`PLE0115`) mark the `global` line.

```py
x = 1

def f():
    x = 1
    def g() -> None:
        nonlocal x
        global x  # TODO: error: [invalid-syntax] "name 'x' is nonlocal and global"
        x = None
```

## Global declaration after `global` statement

```py
def f():
    global x
    y = x
    x = 1  # No error.

x = 2
```

## Semantic syntax errors

Using a name prior to its `global` declaration in the same scope is a syntax error.

```py
def f():
    print(x)
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    global x
    print(x)
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    print(x)
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    global x, y
    print(x)
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    x = 1
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    x = 1

def f():
    global x
    x = 1
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    x = 1

def f():
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x, y
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    del x
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x
    del x
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x, y
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    print(f"{x=}")
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"

# still an error in module scope
x = None
global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
```

## Local bindings override preceding `global` bindings

```py
x = 42

def f():
    global x
    reveal_type(x)  # revealed: Unknown | Literal[42]
    x = "56"
    reveal_type(x)  # revealed: Literal["56"]
```

## Local assignment prevents falling back to the outer scope

```py
x = 42

def f():
    # error: [unresolved-reference] "Name `x` used when not defined"
    reveal_type(x)  # revealed: Unknown
    x = "56"
    reveal_type(x)  # revealed: Literal["56"]
```

## Annotating a `global` binding is a syntax error

```py
x: int = 1

def f():
    global x
    x: str = "foo"  # TODO: error: [invalid-syntax] "annotated name 'x' can't be global"
```
