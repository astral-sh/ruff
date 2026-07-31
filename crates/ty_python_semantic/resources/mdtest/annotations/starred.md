# Starred expression annotations

```toml
[environment]
python-version = "3.11"
```

An unpacked type variable tuple keeps the types of positional arguments passed to `*args`.

```py
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Ts")

def append_int(*args: *Ts) -> tuple[*Ts, int]:
    reveal_type(args)  # revealed: tuple[*Ts@append_int]

    return (*args, 1)

reveal_type(append_int(True, "a"))  # revealed: tuple[Literal[True], Literal["a"], int]
reveal_type(append_int())  # revealed: tuple[int]
```

A concrete starred tuple checks its fixed first argument, remaining argument types, and arity.

```py
def first_arg_int(*args: *tuple[int, *tuple[str, ...]]): ...

first_arg_int(42, "42", "42")  # fine
# error: [invalid-argument-type] "Argument to function `first_arg_int` is incorrect: Expected `tuple[int, *tuple[str, ...]]`, found `tuple[Literal["not an int"], Literal["42"], Literal["42"]]`"
first_arg_int("not an int", "42", "42")
# error: [invalid-argument-type] "Argument to function `first_arg_int` is incorrect: Expected `tuple[int, *tuple[str, ...]]`, found `tuple[Literal[56], Literal["42"], Literal[56]]`"
first_arg_int(56, "42", 56)
# error: [invalid-argument-type] "Argument to function `first_arg_int` is incorrect: Expected `tuple[int, *tuple[str, ...]]`, found `tuple[()]`"
first_arg_int()
```

Open splats can provide fixed prefixes, suffixes, or both, with arguments after the splat.

```py
def prefix(*args: *tuple[str, *tuple[str, ...]]) -> None: ...
def suffix(*args: *tuple[*tuple[str, ...], str]) -> None: ...
def bounded(*args: *tuple[str, *tuple[str, ...], str]) -> None: ...
def execute(*args: *tuple[str, *tuple[str, ...], dict[str, str]]) -> None: ...
def check_valid(
    strings: list[str],
    fixed: tuple[str, ...],
    env: dict[str, str],
) -> None:
    prefix(*strings)
    prefix(*fixed)
    suffix(*strings)
    bounded(*strings)
    execute(*strings, env)
    execute(*fixed, env)
```

A splat with incompatible elements reports the whole-tuple error on the splatted argument.

```py
def check_invalid(invalid: list[int], env: dict[str, str]) -> None:
    # error: [invalid-argument-type] "Argument to function `execute` is incorrect: Expected `tuple[str, *tuple[str, ...], dict[str, str]]`, found `tuple[int, *tuple[int, ...], dict[str, str]]`"
    execute(*invalid, env)
```

An iterable of `Never` values must be empty, so it cannot supply a required prefix or suffix.

```py
from typing import Never

def check_empty(empty: list[Never]) -> None:
    # error: [invalid-argument-type] "Argument to function `prefix` is incorrect: Expected `tuple[str, *tuple[str, ...]]`, found `tuple[()]`"
    prefix(*empty)
```

A runtime type alias preserves the empty-iterable behavior of `Never`.

```py
from typing_extensions import TypeAliasType

RuntimeBottom = TypeAliasType("RuntimeBottom", Never)

def check_aliases(runtime_aliased_empty: list[RuntimeBottom]) -> None:
    # error: [invalid-argument-type] "Argument to function `prefix` is incorrect: Expected `tuple[str, *tuple[str, ...]]`, found `tuple[RuntimeBottom, ...]`"
    prefix(*runtime_aliased_empty)
```
