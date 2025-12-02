# PEP 695 `ParamSpec`

`ParamSpec` was introduced in Python 3.12 while the support for specifying defaults was added in
Python 3.13.

```toml
[environment]
python-version = "3.13"
```

## Definition

```py
def foo1[**P]() -> None:
    reveal_type(P)  # revealed: typing.ParamSpec
```

## Bounds and constraints

`ParamSpec`, when defined using the new syntax, does not allow defining bounds or constraints.

TODO: This results in a lot of syntax errors mainly because the AST doesn't accept them in this
position. The parser could do a better job in recovering from these errors.

<!-- blacken-docs:off -->

```py
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
def foo[**P: int]() -> None:
    # error: [invalid-syntax]
    # error: [invalid-syntax]
    pass
```

<!-- blacken-docs:on -->

## Default

The default value for a `ParamSpec` can be either a list of types, `...`, or another `ParamSpec`.

```py
def foo2[**P = ...]() -> None:
    reveal_type(P)  # revealed: typing.ParamSpec

def foo3[**P = [int, str]]() -> None:
    reveal_type(P)  # revealed: typing.ParamSpec

def foo4[**P, **Q = P]():
    reveal_type(P)  # revealed: typing.ParamSpec
    reveal_type(Q)  # revealed: typing.ParamSpec
```

Other values are invalid.

```py
# error: [invalid-paramspec]
def foo[**P = int]() -> None:
    pass
```
