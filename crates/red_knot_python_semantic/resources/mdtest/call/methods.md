# Methods

## Background: Functions as descriptors

Say we have a simple class `C` with a function definition `f` inside its body:

```py
class C:
    def f(self, x: int) -> str:
        return 1
```

Whenever we access the `f` attribute through the class object itself (`C.f`) or through an instance
(`C().f`), this access happens via the descriptor protocol. Functions are (non-data) descriptors
because they implement a `__get__` method. This is crucial in making sure that method calls work as
expected. In general, the signature of the `__get__` method in the descriptor protocol is
`__get__(self, instance, owner)`. The `self` argument is the descriptor object itself (`f`). The
passed value for the `instance` argument depends on whether the attribute is accessed from the class
object (in which case it is `None`), or from an instance (in which case it is the instance of type
`C`). The `owner` argument is the class itself (`C` of type `Literal[C]`). To summarize:

- `C.f` is equivalent to `C.__dict__["f"].__get__(None, C)`
- `C().f` is equivalent to `C.__dict__["f"].__get__(C(), C)`

The way the special `__get__` method *on functions* works like is as follows. In the former case, if
the `instance` attribute is `None`, it simply returns the function itself:

```py
reveal_type(C.f)  # revealed: Literal[f]
```

However, if the `instance` attribute is not `None`, the `__get__` method returns a *bound method*
object:

```py
reveal_type(C().f)  # revealed: <bound method: `f` of `C`>
```

A bound method is a callable object that contains a reference to the `instance` that it was called
on (can be inspected via `__self__`), and the function object that it refers to (can be inspected
via `__func__`):

```py
bound_method = C().f

reveal_type(bound_method.__self__)  # revealed: C
reveal_type(bound_method.__func__)  # revealed: Literal[f]
```

When we call the bound method, the `instance` is implicitly passed as the first argument (`self`):

```py
reveal_type(C().f(1))  # revealed: str
reveal_type(bound_method(1))  # revealed: str
```

When we call the function object itself, we need to pass the `instance` explicitly:

```py
C.f(1)  # error: [missing-argument]

reveal_type(C.f(C(), 1))  # revealed: str
```

When we access methods from derived classes, they will be bound to instances of the derived class:

```py
class D(C):
    pass

reveal_type(D().f)  # revealed: <bound method: `f` of `D`>
```

If we access an attribute on a bound method object itself, it will defer to `types.MethodType`:

```py
reveal_type(bound_method.__hash__)  # revealed: <bound method: `__hash__` of `MethodType`>
```

## Method calls on literals

### Boolean literals

```py
reveal_type(True.bit_length())  # revealed: int
reveal_type(True.as_integer_ratio())  # revealed: tuple[int, Literal[1]]
```

### Integer literals

```py
reveal_type((42).bit_length())  # revealed: int
```

### String literals

```py
reveal_type("abcde".find("abc"))  # revealed: int
reveal_type("foo".encode(encoding="utf-8"))  # revealed: bytes

"abcde".find(123)  # error: [invalid-argument-type]
```

### Bytes literals

```py
reveal_type(b"abcde".startswith(b"abc"))  # revealed: bool
```

## Method calls on `LiteralString`

```py
from typing_extensions import LiteralString

def f(s: LiteralString) -> None:
    reveal_type(s.find("a"))  # revealed: int
```

## Method calls on `tuple`

```py
def f(t: tuple[int, str]) -> None:
    reveal_type(t.index("a"))  # revealed: int
```

## Method calls on unions

```py
from typing import Any

class A:
    def f(self) -> int:
        return 1

class B:
    def f(self) -> str:
        return "a"

def f(a_or_b: A | B, any_or_a: Any | A):
    reveal_type(a_or_b.f)  # revealed: <bound method: `f` of `A`> | <bound method: `f` of `B`>
    reveal_type(a_or_b.f())  # revealed: int | str

    reveal_type(any_or_a.f)  # revealed: Any | <bound method: `f` of `A`>
    reveal_type(any_or_a.f())  # revealed: Any | int
```

## Method calls on `KnownInstance` types

```toml
[environment]
python-version = "3.12"
```

```py
type IntOrStr = int | str

reveal_type(IntOrStr.__or__)  # revealed: <bound method: `__or__` of `typing.TypeAliasType`>
```

## `__get__` on normal functions

```py
def f(x: int) -> str:
    return "a"

reveal_type(f.__get__)  # revealed: <method-wrapper: `f`>
reveal_type(f.__get__(None, f))  # revealed: Literal[f]
reveal_type(f.__get__(None, f)(1))  # revealed: str

# Fallback to MethodWrapperType
reveal_type(f.__get__.__hash__())  # revealed: int
```
