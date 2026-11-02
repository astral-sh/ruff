# Subscripts involving type aliases

Aliases are expanded during analysis of subscripts.

```toml
[environment]
python-version = "3.12"
```

## Non-recursive aliases

```py
from typing_extensions import TypeAlias, Literal

ImplicitTuple = tuple[str, int, int]
PEP613Tuple: TypeAlias = tuple[str, int, int]
type PEP695Tuple = tuple[str, int, int]

ImplicitZero = Literal[0]
PEP613Zero: TypeAlias = Literal[0]
type PEP695Zero = Literal[0]

def f(
    implicit_tuple: ImplicitTuple,
    pep_613_tuple: PEP613Tuple,
    pep_695_tuple: PEP695Tuple,
    implicit_zero: ImplicitZero,
    pep_613_zero: PEP613Zero,
    pep_695_zero: PEP695Zero,
    invalid_bound: float,
):
    reveal_type(implicit_tuple[:2])  # revealed: tuple[str, int]
    reveal_type(implicit_tuple[implicit_zero])  # revealed: str
    reveal_type(implicit_tuple[pep_613_zero])  # revealed: str
    reveal_type(implicit_tuple[pep_695_zero])  # revealed: str
    implicit_tuple[invalid_bound:]  # error: [invalid-argument-type]

    reveal_type(pep_613_tuple[:2])  # revealed: tuple[str, int]
    reveal_type(pep_613_tuple[implicit_zero])  # revealed: str
    reveal_type(pep_613_tuple[pep_613_zero])  # revealed: str
    reveal_type(pep_613_tuple[pep_695_zero])  # revealed: str
    pep_613_tuple[invalid_bound:]  # error: [invalid-argument-type]

    reveal_type(pep_695_tuple[:2])  # revealed: tuple[str, int]
    reveal_type(pep_695_tuple[implicit_zero])  # revealed: str
    reveal_type(pep_695_tuple[pep_613_zero])  # revealed: str
    reveal_type(pep_695_tuple[pep_695_zero])  # revealed: str
    pep_695_tuple[invalid_bound:]  # error: [invalid-argument-type]
```

## Recursive containers

Indexing a recursive list can produce either another list or a string. A subsequent string key is
invalid for both alternatives, with one diagnostic for each. Both alias forms report the same
errors.

```py
Value = str | list["Value"]
type ExplicitValue = str | list[ExplicitValue]

def implicit(value: Value):
    # error: [invalid-argument-type] "on object of type `str`"
    # error: [invalid-argument-type] "on object of type `list[Value]`"
    value[0]["checksum"]

def explicit(value: ExplicitValue):
    # error: [invalid-argument-type] "on object of type `str`"
    # error: [invalid-argument-type] "on object of type `list[ExplicitValue]`"
    value[0]["checksum"]
```

## Iteration over optional recursive containers

Iterating over a recursive list or string produces the same alternatives as indexing it. `None`
additionally prevents iteration and subscripting.

```py
Value = str | list["Value | None"]
type ExplicitValue = str | list[ExplicitValue | None]

def implicit(value: dict[str, Value | None]):
    # error: [invalid-argument-type] "on object of type `str`"
    # error: [invalid-argument-type] "on object of type `list[Value | None]`"
    # error: [not-subscriptable]
    # error: [not-iterable]
    [element["location"] for element in value["output"]]

def explicit(value: dict[str, ExplicitValue | None]):
    # error: [invalid-argument-type] "on object of type `str`"
    # error: [invalid-argument-type] "on object of type `list[ExplicitValue | None]`"
    # error: [not-subscriptable]
    # error: [not-iterable]
    [element["location"] for element in value["output"]]
```

## Recursive path traversal

A path of string and integer keys cannot traverse arbitrary dictionaries and strings. Each
incompatible combination is reported once, including after reassignment within a loop.

```py
Value = dict[str, "Value"] | str
type ExplicitValue = dict[str, ExplicitValue] | str

def implicit(parent: Value, path: tuple[str | int, ...]):
    for segment in path:
        # error: [invalid-argument-type] "on object of type `dict[str, Value]`"
        # error: [invalid-argument-type] "on object of type `str`"
        parent = parent[segment]

def explicit(parent: ExplicitValue, path: tuple[str | int, ...]):
    for segment in path:
        # error: [invalid-argument-type] "on object of type `dict[str, ExplicitValue]`"
        # error: [invalid-argument-type] "on object of type `str`"
        parent = parent[segment]
```

## Recursive keys

Overlapping alternatives in a key type also produce just one diagnostic for each invalid key type.

```py
Key = str | list["Key"]
type ExplicitKey = str | list[ExplicitKey]

def implicit(value: list[int], key: Key | str):
    # error: [invalid-argument-type] "with key of type `str`"
    # error: [invalid-argument-type] "with key of type `list[Key]`"
    value[key]

def explicit(value: list[int], key: ExplicitKey | str):
    # error: [invalid-argument-type] "with key of type `str`"
    # error: [invalid-argument-type] "with key of type `list[ExplicitKey]`"
    value[key]
```
