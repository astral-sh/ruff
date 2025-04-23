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
    # TODO: error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    x = ""
```

## Nested intervening scope

A `global` statement causes lookup to skip any bindings in intervening scopes:

```py
x = 1

def outer():
    x = ""

    def inner():
        global x
        # TODO should be `Unknown | Literal[1]`
        reveal_type(x)  # revealed: Unknown | Literal[""]
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
        global x  # TODO error: [invalid-syntax] "name 'x' is nonlocal and global"
        x = None
```

## Global declaration after `global` statement

```py
def f():
    global x
    # TODO this should also not be an error
    y = x  # error: [unresolved-reference] "Name `x` used when not defined"
    x = 1  # No error.

x = 2
```

## Semantic syntax errors

TODO: these cases are from the `PLE0118` `ruff` tests and should all cause
`load-before-global-declaration` errors.

```py
x = 1

def f():
    print(x)
    global x
    print(x)

def f():
    global x
    print(x)
    global x
    print(x)

def f():
    print(x)
    global x, y
    print(x)

def f():
    global x, y
    print(x)
    global x, y
    print(x)

def f():
    x = 1
    global x
    x = 1

def f():
    global x
    x = 1
    global x
    x = 1

def f():
    del x
    global x, y
    del x

def f():
    global x, y
    del x
    global x, y
    del x

def f():
    del x
    global x
    del x

def f():
    global x
    del x
    global x
    del x

def f():
    del x
    global x, y
    del x

def f():
    global x, y
    del x
    global x, y
    del x

def f():
    print(f"{x=}")
    global x

# still an error in module scope
x = None
global x
```
