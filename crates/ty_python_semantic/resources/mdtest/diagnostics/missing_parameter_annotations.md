# Missing parameter annotations

```toml
[rules]
missing-type-function-argument = "error"
missing-type-args = "error"
missing-type-kwargs = "error"
```

## Parameter kinds

```py
def f(
    positional,  # error: [missing-type-function-argument]
    /,
    named=0,  # snapshot: missing-type-function-argument
    *args,  # snapshot: missing-type-args
    keyword,  # error: [missing-type-function-argument]
    **kwargs,  # snapshot: missing-type-kwargs
): ...
```

```snapshot
error[missing-type-function-argument]: Missing type annotation for parameter `named`
 --> src/mdtest_snippet.py:4:5
  |
4 |     named=0,  # snapshot: missing-type-function-argument
  |     ^^^^^


error[missing-type-args]: Missing type annotation for parameter `*args`
 --> src/mdtest_snippet.py:5:5
  |
5 |     *args,  # snapshot: missing-type-args
  |     ^^^^^


error[missing-type-kwargs]: Missing type annotation for parameter `**kwargs`
 --> src/mdtest_snippet.py:7:5
  |
7 |     **kwargs,  # snapshot: missing-type-kwargs
  |     ^^^^^^^^
```

## Method receivers

The first positional parameter of an instance method or class method is exempt regardless of its
name. In a static method, every parameter requires an annotation.

```py
class C:
    def method(receiver, /, value): ...  # error: [missing-type-function-argument] "parameter `value`"
    @classmethod
    def method_on_class(receiver): ...  # ok
    @staticmethod
    def static(self): ...  # error: [missing-type-function-argument] "parameter `self`"
    def __new__(cls):  # ok
        return super().__new__(cls)
```

When `*args` receives the instance, both `*args` and the keyword-only parameters require
annotations.

```py
class Variadic:
    def method(
        *args,  # error: [missing-type-args]
        flag,  # error: [missing-type-function-argument]
    ): ...
```

## Overloads

The overload signatures describe the parameters that callers can pass. Their implementation may omit
annotations.

```py
from typing import overload

@overload
def get(key: int, default) -> int: ...  # error: [missing-type-function-argument]
@overload
def get(key: str) -> str: ...
def get(key, default=None): ...  # ok
```
