# Missing return annotations

```toml
[rules]
missing-return-type-public-function = "error"
missing-return-type-private-function = "error"
missing-return-type-special-method = "error"
missing-return-type-static-method = "error"
missing-return-type-class-method = "error"
```

## Functions

```py
def increment(value: int):  # snapshot: missing-return-type-public-function
    return value + 1

def _helper():  # snapshot: missing-return-type-private-function
    return "ready"

def annotated() -> str:  # ok
    return "ready"
```

```snapshot
error[missing-return-type-public-function]: Missing return type annotation for public function `increment`
 --> src/mdtest_snippet.py:1:5
  |
1 | def increment(value: int):  # snapshot: missing-return-type-public-function
  |     ^^^^^^^^^


error[missing-return-type-private-function]: Missing return type annotation for private function `_helper`
 --> src/mdtest_snippet.py:4:5
  |
4 | def _helper():  # snapshot: missing-return-type-private-function
  |     ^^^^^^^
```

## Methods

Class and static methods have their own rules, even when their names are private.

```py
class Counter:
    def __init__(self):  # snapshot: missing-return-type-special-method
        self.value = 0

    @staticmethod
    def _parse(text: str):  # snapshot: missing-return-type-static-method
        return int(text)

    @classmethod
    def _unit(cls):  # snapshot: missing-return-type-class-method
        return 1
```

```snapshot
error[missing-return-type-special-method]: Missing return type annotation for special method `__init__`
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __init__(self):  # snapshot: missing-return-type-special-method
  |         ^^^^^^^^


error[missing-return-type-static-method]: Missing return type annotation for static method `_parse`
 --> src/mdtest_snippet.py:6:9
  |
6 |     def _parse(text: str):  # snapshot: missing-return-type-static-method
  |         ^^^^^^


error[missing-return-type-class-method]: Missing return type annotation for class method `_unit`
  --> src/mdtest_snippet.py:10:9
   |
10 |     def _unit(cls):  # snapshot: missing-return-type-class-method
   |         ^^^^^
```

## Implicit class and static methods

```py
class Base:
    def __new__(cls): ...  # error: [missing-return-type-static-method]
    def __init_subclass__(cls): ...  # error: [missing-return-type-class-method]
```

## Visibility

A nested function, a member of a private class, a property setter, and a function omitted from
`__all__` are all private.

```py
__all__ = ["Public"]

class Public:
    @property
    def size(self):  # error: [missing-return-type-public-function]
        return 1

    @size.setter
    def size(self, value: int): ...  # error: [missing-return-type-private-function]

class _Private:
    def method(self): ...  # error: [missing-return-type-private-function]

def helper():  # error: [missing-return-type-private-function]
    def nested(): ...  # error: [missing-return-type-private-function]
```

## Private modules

`_internal/api.py`:

```py
def run(): ...  # error: [missing-return-type-private-function]
```

## Overloads

Only the overload signatures require an annotation.

```py
from typing import overload

@overload
def parse(value: int): ...  # snapshot: missing-return-type-public-function
@overload
def parse(value: str) -> str: ...
def parse(value):  # ok
    return value
```

```snapshot
error[missing-return-type-public-function]: Missing return type annotation for public function `parse`
 --> src/mdtest_snippet.py:4:5
  |
4 | def parse(value: int): ...  # snapshot: missing-return-type-public-function
  |     ^^^^^
```
