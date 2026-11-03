# Missing return annotations

```toml
[rules]
missing-return-annotation = "error"
```

## Functions

```py
def increment(value: int):  # snapshot: missing-return-annotation
    return value + 1

def annotated() -> str:  # ok
    return "ready"
```

```snapshot
error[missing-return-annotation]: Missing return type annotation for function `increment`
 --> src/mdtest_snippet.py:1:5
  |
1 | def increment(value: int):  # snapshot: missing-return-annotation
  |     ^^^^^^^^^
```

## Methods

```py
class Counter:
    def __init__(self):  # error: [missing-return-annotation]
        self.value = 0

    @classmethod
    def _unit(cls): ...  # error: [missing-return-annotation]
```

## Overloads

Only the overload signatures require an annotation.

```py
from typing import overload

@overload
def parse(value: int): ...  # error: [missing-return-annotation]
@overload
def parse(value: str) -> str: ...  # ok
def parse(value):  # ok
    return value
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
