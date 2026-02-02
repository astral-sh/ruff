# Subscript assignment diagnostics

<!-- snapshot-diagnostics -->

## Invalid value type

### For a `list`

```py
numbers: list[int] = []
numbers[0] = "three"  # error: [invalid-assignment]
```

### For a `dict`

```py
config: dict[str, int] = {}
config["retries"] = "three"  # error: [invalid-assignment]
```

## Invalid key type

### For a `list`

```py
numbers: list[int] = []
numbers["zero"] = 3  # error: [invalid-assignment]
```

### For a `dict`

```py
config: dict[str, int] = {}
config[0] = 3  # error: [invalid-assignment]
```

## Invalid value type for `TypedDict`

```py
from typing import TypedDict

class Config(TypedDict):
    retries: int

def _(config: Config) -> None:
    config["retries"] = "three"  # error: [invalid-assignment]
```

## Invalid key type for `TypedDict`

```py
from typing import TypedDict

class Config(TypedDict):
    retries: int

def _(config: Config) -> None:
    config[0] = 3  # error: [invalid-key]
```

## Misspelled key for `TypedDict`

```py
from typing import TypedDict

class Config(TypedDict):
    retries: int

def _(config: Config) -> None:
    config["Retries"] = 30.0  # error: [invalid-key]
```

## No `__setitem__` method

```py
class ReadOnlyDict:
    def __getitem__(self, key: str) -> int:
        return 42

config = ReadOnlyDict()
config["retries"] = 3  # error: [invalid-assignment]
```

## Possibly missing `__setitem__` method

```py
def _(config: dict[str, int] | None) -> None:
    config["retries"] = 3  # error: [invalid-assignment]
```

## Unknown key for one element of a union

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    phone_number: str

class Animal(TypedDict):
    name: str
    legs: int

def _(being: Person | Animal) -> None:
    being["legs"] = 4  # error: [invalid-key]
```

## Unknown key for all elements of a union

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    phone_number: str

class Animal(TypedDict):
    name: str
    legs: int

def _(being: Person | Animal) -> None:
    # error: [invalid-key]
    # error: [invalid-key]
    being["surname"] = "unknown"
```

## Wrong value type for one element of a union

```py
def _(config: dict[str, int] | dict[str, str]) -> None:
    config["retries"] = 3  # error: [invalid-assignment]
```

## Wrong value type for all elements of a union

```py
def _(config: dict[str, int] | dict[str, str]) -> None:
    # error: [invalid-assignment]
    # error: [invalid-assignment]
    config["retries"] = 3.0
```
