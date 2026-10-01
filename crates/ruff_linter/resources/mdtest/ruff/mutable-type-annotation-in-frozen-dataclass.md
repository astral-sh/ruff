# `mutable-type-annotation-in-frozen-dataclass` (`RUF078`)

```toml
lint.preview = true
lint.select = ["RUF078"]
```

## Basic errors

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class SomeDataClass1:
    list1: list[int]    # snapshot: mutable-type-annotation-in-frozen-dataclass

@dataclass(frozen=True)
class SomeDataClass2:
    dict1: dict[int, str]   # snapshot: mutable-type-annotation-in-frozen-dataclass

@dataclass(frozen=True)
class SomeDataClass3:
    set1: set[int]  # snapshot: mutable-type-annotation-in-frozen-dataclass
  
@dataclass(frozen=True)
class SomeDataClass4:
    tuple1: tuple[int]

```

```snapshot
error[RUF078]: Do not use mutable type annotations in a frozen dataclass
 --> src/mdtest_snippet.py:5:12
  |
5 |     list1: list[int]    # snapshot: mutable-type-annotation-in-frozen-dataclass
  |            ^^^^^^^^^


error[RUF078]: Do not use mutable type annotations in a frozen dataclass
 --> src/mdtest_snippet.py:9:12
  |
9 |     dict1: dict[int, str]   # snapshot: mutable-type-annotation-in-frozen-dataclass
  |            ^^^^^^^^^^^^^^


error[RUF078]: Do not use mutable type annotations in a frozen dataclass
  --> src/mdtest_snippet.py:13:11
   |
13 |     set1: set[int]  # snapshot: mutable-type-annotation-in-frozen-dataclass
   |           ^^^^^^^^
```

## No errors
A dataclass that is not frozen can hold mutable members

```py
from dataclasses import dataclass

@dataclass
class UnfrozenDataclass:
    list1: list[int]    # no diagnostic
```

A frozen dataclass having only immutable members

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class FrozenDataclassWithImmutableMembers:
    tuple1: tuple[int]    # no diagnostic
    set1: frozenset[int]    # no diagnostic
```

## Known Limitations

For Python versions < 3.15, frozendict is not defined. As a result this lint will still raise a diagnostic in such versions, and hence the tests don't cover this yet.
It is recommended to set the pyproject.toml to use requires-python = ">=3.15".