# Builtin scope

## Conditional local override of builtin

If a builtin name is conditionally shadowed by a local variable, the function's binding scope
terminates name resolution. The name can be unbound, but it cannot refer to the builtin:

```py
def _(flag: bool) -> None:
    if flag:
        abs = 1
        chr: int = 1

    # error: [possibly-unresolved-reference]
    reveal_type(abs)  # revealed: Literal[1]
    # error: [possibly-unresolved-reference]
    reveal_type(chr)  # revealed: Literal[1]
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
