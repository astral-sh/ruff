# Missing return annotations

```toml
[rules]
missing-return-annotation = "error"
```

## Functions

Enabling the rule alone leaves the return type seen by callers as `Unknown`.

```py
def increment(value: int):  # snapshot: missing-return-annotation
    return value + 1

reveal_type(increment(1))  # revealed: Unknown

def annotated() -> str:  # ok
    return "ready"
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `increment`
 --> src/mdtest_snippet.py:1:5
  |
1 | def increment(value: int):  # snapshot: missing-return-annotation
  |     ^^^^^^^^^
help: Add return type annotation `int`
  |
  - def increment(value: int):  # snapshot: missing-return-annotation
1 + def increment(value: int) -> int:  # snapshot: missing-return-annotation
2 |     return value + 1
  |
note: This is an unsafe fix and may change runtime behavior
```

## Methods

```py
class Counter:
    def __init__(self):  # snapshot: missing-return-annotation
        self.value = 0

    @classmethod
    def _unit(cls): ...  # error: [missing-return-annotation]
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `__init__`
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __init__(self):  # snapshot: missing-return-annotation
  |         ^^^^^^^^
help: Add return type annotation `None`
  |
1 | class Counter:
  -     def __init__(self):  # snapshot: missing-return-annotation
2 +     def __init__(self) -> None:  # snapshot: missing-return-annotation
3 |         self.value = 0
  |
note: This is an unsafe fix and may change runtime behavior
```

## Overloads

Only the overload signatures require an annotation.

```py
from typing import overload

@overload
def parse(value: int): ...  # snapshot: missing-return-annotation
@overload
def parse(value: str) -> str: ...  # ok
def parse(value):  # ok
    return value
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `parse`
 --> src/mdtest_snippet.py:4:5
  |
4 | def parse(value: int): ...  # snapshot: missing-return-annotation
  |     ^^^^^
```

## Return type inference

The rule requires an explicit annotation whether ty infers a precise return type or falls back to
`Unknown`.

```toml
[analysis]
return-type-inference = "always"

[rules]
missing-return-annotation = "error"
```

```py
def inferred():  # error: [missing-return-annotation]
    return 1

def echo(value):  # error: [missing-return-annotation]
    return value

reveal_type(inferred())  # revealed: Literal[1]
reveal_type(echo(1))  # revealed: Unknown
```

## Imported return type

`models.py`:

```py
class Item: ...

def load() -> Item:
    return Item()
```

`client.py`:

```py
from models import load

def item():  # snapshot: missing-return-annotation
    return load()
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `item`
 --> src/client.py:3:5
  |
3 | def item():  # snapshot: missing-return-annotation
  |     ^^^^
help: Add return type annotation `Item`
  |
  - from models import load
1 + from models import load, Item
2 |
  - def item():  # snapshot: missing-return-annotation
3 + def item() -> Item:  # snapshot: missing-return-annotation
4 |     return load()
  |
note: This is an unsafe fix and may change runtime behavior
```

## Annotations on older Python versions

On Python 3.9, an inferred union needs to be quoted so the annotation can be evaluated at runtime.

```toml
[environment]
python-version = "3.9"

[rules]
missing-return-annotation = "error"
```

```py
str = 0

def label(active: bool):  # snapshot: missing-return-annotation
    if active:
        return "active"
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `label`
 --> src/mdtest_snippet.py:3:5
  |
3 | def label(active: bool):  # snapshot: missing-return-annotation
  |     ^^^^^
help: Add return type annotation `'builtins.str | None'`
  |
1 + import builtins
2 | str = 0
3 |
  - def label(active: bool):  # snapshot: missing-return-annotation
4 + def label(active: bool) -> 'builtins.str | None':  # snapshot: missing-return-annotation
5 |     if active:
  |
note: This is an unsafe fix and may change runtime behavior
```

## Incompatible inherited return type

When an override returns a value incompatible with its inherited annotation, ty leaves the choice of
annotation to the author.

```py
class Base:
    def value(self) -> str:
        return "value"

class Child(Base):
    def value(self):  # snapshot: missing-return-annotation
        return 1
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `value`
 --> src/mdtest_snippet.py:6:9
  |
6 |     def value(self):  # snapshot: missing-return-annotation
  |         ^^^^^
```

## Conditional type definitions

An annotation evaluated at function definition time can refer to a class only available on some
paths. In that case ty reports the missing annotation without suggesting the class name.

```py
from random import random

if random() < 0.5:
    class Item: ...

def load():  # snapshot: missing-return-annotation
    return Item()
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `load`
 --> src/mdtest_snippet.py:6:5
  |
6 | def load():  # snapshot: missing-return-annotation
  |     ^^^^
```
