# Starred expression annotations

```toml
[environment]
python-version = "3.11"
```

Type annotations for `*args` can be starred expressions themselves:

```py
from typing import Never, TypeAlias
from typing_extensions import TypeAliasType, TypeVarTuple

Ts = TypeVarTuple("Ts")
Bottom: TypeAlias = Never
RuntimeBottom = TypeAliasType("RuntimeBottom", Never)

def append_int(*args: *Ts) -> tuple[*Ts, int]:
    reveal_type(args)  # revealed: tuple[*Ts@append_int]

    return (*args, 1)

reveal_type(append_int(True, "a"))  # revealed: tuple[Literal[True], Literal["a"], int]
reveal_type(append_int())  # revealed: tuple[int]

def first_arg_int(*args: *tuple[int, *tuple[str, ...]]): ...

first_arg_int(42, "42", "42")  # fine
# error: [invalid-argument-type]
first_arg_int("not an int", "42", "42")
# error: [invalid-argument-type]
first_arg_int(56, "42", 56)
# error: [invalid-argument-type]
first_arg_int()

def prefix(*args: *tuple[str, *tuple[str, ...]]) -> None: ...
def suffix(*args: *tuple[*tuple[str, ...], str]) -> None: ...
def bounded(*args: *tuple[str, *tuple[str, ...], str]) -> None: ...
def execute(*args: *tuple[str, *tuple[str, ...], dict[str, str]]) -> None: ...
def check(
    strings: list[str],
    fixed: tuple[str, ...],
    invalid: list[int],
    empty: list[Never],
    aliased_empty: list[Bottom],
    runtime_aliased_empty: list[RuntimeBottom],
    env: dict[str, str],
) -> None:
    prefix(*strings)
    prefix(*fixed)
    suffix(*strings)
    bounded(*strings)
    execute(*strings, env)
    execute(*fixed, env)

    execute(
        *invalid,  # error: [invalid-argument-type]
        env,
    )

    prefix(
        *empty,  # error: [invalid-argument-type]
    )
    prefix(
        *aliased_empty,  # error: [invalid-argument-type]
    )
    prefix(
        *runtime_aliased_empty,  # error: [invalid-argument-type]
    )
    suffix(
        *empty,  # error: [invalid-argument-type]
    )
    bounded(
        *empty,  # error: [invalid-argument-type]
    )
    execute(
        *empty,  # error: [invalid-argument-type]
        env,
    )
```
