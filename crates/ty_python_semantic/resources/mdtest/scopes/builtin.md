# Builtin scope

## Conditional local override of builtin

If a builtin name is conditionally shadowed by a local variable, a name lookup should union the
builtin type with the conditionally-defined type:

```py
def _(flag: bool) -> None:
    if flag:
        abs = 1
        chr: int = 1

    reveal_type(abs)  # revealed: Literal[1] | (def abs[_T](x: SupportsAbs[_T@abs], /) -> _T@abs)
    reveal_type(chr)  # revealed: Literal[1] | (def chr(i: SupportsIndex, /) -> str)
```

## Conditionally global override of builtin

If a builtin name is conditionally shadowed by a global variable, a name lookup should union the
builtin type with the conditionally-defined type:

```py
def flag() -> bool:
    return True

if flag():
    abs = 1
    chr: int = 1

def _():
    # TODO: Should ideally be `Literal[1] | (def abs(x: SupportsAbs[_T], /) -> _T)`
    reveal_type(abs)  # revealed: Literal[1]
    # TODO: Should ideally be `int | (def chr(i: SupportsIndex, /) -> str)`
    reveal_type(chr)  # revealed: int
```
