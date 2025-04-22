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
    # @Todo(error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`")
    x = ""
```

## Nested intervening scope

TODO this should give the outer module type of `Unknown | Literal[1]`, not `Literal[""]`

```py
x = 1

def outer():
    x = ""

    def inner():
        global x
        reveal_type(x)  # revealed: Unknown | Literal[""]
```

## Narrowing

An assignment following a `global` statement should narrow the type in the local scope after the
assignment. The revealed type of `Literal[1]` here is in line with [pyright], while [mypy] reports
`builtins.int`.

```py
x: int | None

def f():
    global x
    x = 1
    reveal_type(x)  # revealed: Literal[1]
```

This related case is adapted from a `mypy` [test][t] with the comment:

> This is unsafe, but we don't generate an error, for convenience. Besides, this is probably a very
> rare case.

```py
g: str | None

def f():
    global g
    g = "x"
    def nested() -> str:
        return g
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

This is also adapted from a `mypy` [test].

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

[mypy]: https://mypy-play.net/?mypy=latest&python=3.12&gist=84f45a50e34d0426db26f5f57449ab98
[pyright]: https://pyright-play.net/?pythonVersion=3.8&strict=true&code=B4LgBAlgdgLmA%2BYByB7KBTAUJgJugZmPgBQCUImYVYA5gDYoBGAhnWMJdcGALxgCMnKgCd0AN3SsA%2BjACeAB3TFgpKgGIwoia3Q5wAGQgx0w1gG1%2BAXUxA
[t]: https://github.com/python/mypy/blob/master/test-data/unit/check-optional.test#L1134
[test]: https://github.com/python/mypy/blob/master/test-data/unit/check-possibly-undefined.test#L194
