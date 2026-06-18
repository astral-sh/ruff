# PEP 695 `TypeVarTuple`

## Definition and validation

```toml
environment.python-version = "3.13"
```

```py
def definition[*Ts](*args: *Ts) -> tuple[*Ts]:
    reveal_type(Ts)  # revealed: TypeVarTuple
    reveal_type(args)  # revealed: tuple[*Ts@definition]
    return args

class Invalid[*Ts, *Us]:  # error: [invalid-type-form]
    pass
```
