# Missing parameter annotations

```toml
[rules]
missing-parameter-annotation = "error"
```

## Parameter kinds

```py
def f(
    positional,  # error: [missing-parameter-annotation]
    /,
    named=0,  # snapshot: missing-parameter-annotation
    *args,  # snapshot: missing-parameter-annotation
    keyword,  # error: [missing-parameter-annotation]
    **kwargs,  # snapshot: missing-parameter-annotation
): ...
```

```snapshot
error[missing-parameter-annotation]: Missing type annotation for parameter `named`
 --> src/mdtest_snippet.py:4:5
  |
4 |     named=0,  # snapshot: missing-parameter-annotation
  |     ^^^^^


error[missing-parameter-annotation]: Missing type annotation for parameter `*args`
 --> src/mdtest_snippet.py:5:5
  |
5 |     *args,  # snapshot: missing-parameter-annotation
  |     ^^^^^


error[missing-parameter-annotation]: Missing type annotation for parameter `**kwargs`
 --> src/mdtest_snippet.py:7:5
  |
7 |     **kwargs,  # snapshot: missing-parameter-annotation
  |     ^^^^^^^^
```

## Method receivers

The first positional parameter of an instance method or class method is exempt regardless of its
name. In a static method, every parameter requires an annotation.

```py
class C:
    def method(receiver, /, value): ...  # error: [missing-parameter-annotation] "parameter `value`"
    @classmethod
    def method_on_class(receiver): ...  # ok
    @staticmethod
    def static(self): ...  # error: [missing-parameter-annotation] "parameter `self`"
    def __new__(cls):  # ok
        return super().__new__(cls)
```

When `*args` receives the instance, both `*args` and the keyword-only parameters require
annotations.

```py
class Variadic:
    def method(
        *args,  # error: [missing-parameter-annotation]
        flag,  # error: [missing-parameter-annotation]
    ): ...
```

## Overloads

The overload signatures describe the parameters that callers can pass. Their implementation may omit
annotations.

```py
from typing import overload

@overload
def get(key: int, default) -> int: ...  # error: [missing-parameter-annotation]
@overload
def get(key: str) -> str: ...
def get(key, default=None): ...  # ok
```
