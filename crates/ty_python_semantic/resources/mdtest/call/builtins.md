# Calling builtins

## `bool` with incorrect arguments

```py
class NotBool:
    __bool__ = None

# error: [too-many-positional-arguments] "Too many positional arguments to class `bool`: expected 1, got 2"
bool(1, 2)

# TODO: We should emit an `unsupported-bool-conversion` error here because the argument doesn't implement `__bool__` correctly.
bool(NotBool())
```

## Calls to `str()`

### Valid calls

```py
str()
str("")
str(b"")
str(1)
str(object=1)

str(b"M\xc3\xbcsli", "utf-8")
str(b"M\xc3\xbcsli", "utf-8", "replace")

str(b"M\x00\xfc\x00s\x00l\x00i\x00", encoding="utf-16")
str(b"M\x00\xfc\x00s\x00l\x00i\x00", encoding="utf-16", errors="ignore")

str(bytearray.fromhex("4d c3 bc 73 6c 69"), "utf-8")
str(bytearray(), "utf-8")
str(memoryview(b"hello world"), "utf-8")

str(encoding="utf-8", object=b"M\xc3\xbcsli")
str(b"", errors="replace")
```

### `range` as an ordinary `range` value

```py
reveal_type(list(range(3)))  # revealed: list[int]
reveal_type([range(0)])  # revealed: list[range]

class Uop:
    replicated = range(0)

def _(uop: Uop) -> None:
    uop.replicated = range(1, 3)
    reveal_type(uop.replicated)  # revealed: range
```

### Invalid calls

```py
# These are valid at runtime, but the typeshed signature for `str.__new__` requires `object`
# when `encoding` or `errors` are provided.
# error: [no-matching-overload]
str(encoding="utf-8")

# error: [no-matching-overload]
str(errors="replace")

# error: [invalid-argument-type]
# error: [invalid-argument-type]
str(1, 2)

# error: [no-matching-overload]
str(o=1)

# First argument is not a bytes-like object:
# error: [invalid-argument-type]
str("Müsli", "utf-8")

# Second argument is not a valid encoding:
# error: [invalid-argument-type]
str(b"M\xc3\xbcsli", b"utf-8")
```

## Calls to `isinstance`

We infer `Literal[True]` for a limited set of cases where we can be sure that the answer is correct,
but fall back to `bool` otherwise.

For tuple targets, this is intentionally limited to fixed, top-level tuples of exact class literals
with the default metaclass. Every member of an ordinary nominal input union must be covered. Nested
or variadic tuples, PEP 604 union members, typing special forms, protocols, custom metaclasses, and
subclass-typed targets remain `bool`, as do flow-narrowed, type-alias, `TypeVar`, and `type[...]`
inputs.

```py
from enum import Enum
from types import FunctionType
from typing import Protocol, TypeVar, runtime_checkable

class Answer(Enum):
    NO = 0
    YES = 1

reveal_type(isinstance(True, bool))  # revealed: Literal[True]
reveal_type(isinstance(True, int))  # revealed: Literal[True]
reveal_type(isinstance(True, object))  # revealed: Literal[True]
reveal_type(isinstance("", str))  # revealed: Literal[True]
reveal_type(isinstance(1, int))  # revealed: Literal[True]
reveal_type(isinstance(b"", bytes))  # revealed: Literal[True]
reveal_type(isinstance(Answer.NO, Answer))  # revealed: Literal[True]

reveal_type(isinstance((1, 2), tuple))  # revealed: Literal[True]

def f(): ...

reveal_type(isinstance(f, FunctionType))  # revealed: Literal[True]

reveal_type(isinstance("", int))  # revealed: bool

class A: ...
class SubclassOfA(A): ...
class OtherSubclassOfA(A): ...
class B: ...

reveal_type(isinstance(A, type))  # revealed: Literal[True]

a = A()

reveal_type(isinstance(a, A))  # revealed: Literal[True]
reveal_type(isinstance(a, object))  # revealed: Literal[True]
reveal_type(isinstance(a, SubclassOfA))  # revealed: bool
reveal_type(isinstance(a, B))  # revealed: bool

s = SubclassOfA()
reveal_type(isinstance(s, SubclassOfA))  # revealed: Literal[True]
reveal_type(isinstance(s, A))  # revealed: Literal[True]

def _(x: A | B, y: list[int]):
    targets = (A, B)

    reveal_type(isinstance(y, list))  # revealed: Literal[True]
    reveal_type(isinstance(x, A))  # revealed: bool
    reveal_type(isinstance(x, (A, B)))  # revealed: Literal[True]
    reveal_type(isinstance(x, (A, (B, bytes))))  # revealed: bool
    reveal_type(isinstance(x, targets))  # revealed: Literal[True]
    reveal_type(isinstance(x, (A, bytes)))  # revealed: bool

    if isinstance(x, A):
        pass
    else:
        reveal_type(x)  # revealed: B & ~A
        reveal_type(isinstance(x, B))  # revealed: Literal[True]

def returns_bool(x: A) -> bool:
    if isinstance(x, (B, A)):
        return True

def returns_bool_union(x: A | B) -> bool:
    if isinstance(x, (A, B)):
        return True

def returns_bool_object(x: object) -> bool:
    if isinstance(x, (object,)):
        return True

def returns_bool_stored_tuple(x: A | B) -> bool:
    targets = (A, B)
    if isinstance(x, targets):
        return True

def partial_targets_are_not_exhaustive(x: A | B) -> bool:  # error: [invalid-return-type]
    if isinstance(x, (A, bytes)):
        return True

def variadic_targets_are_not_exhaustive(x: A, targets: tuple[type[A], ...]) -> bool:  # error: [invalid-return-type]
    if isinstance(x, targets):
        return True

def subclass_targets_are_not_exhaustive(x: A, target: type[A]) -> bool:  # error: [invalid-return-type]
    if isinstance(x, (target,)):
        return True

@runtime_checkable
class RuntimeProtocol(Protocol):
    value: int

class StructuralImplementation:
    value: int

def protocol_targets_are_not_exhaustive(x: StructuralImplementation) -> bool:
    if isinstance(x, (RuntimeProtocol, bytes)):
        return True
    return ""  # error: [invalid-return-type]

class RejectingMeta(type):
    def __instancecheck__(self, instance: object, /) -> bool:
        return False

class RejectingBase(metaclass=RejectingMeta): ...
class RejectingChild(RejectingBase): ...

# `isinstance` truthiness does not model `__instancecheck__` for single-class targets: this is
# inferred as `Literal[True]` from nominal subtyping, even though it returns `False` at runtime.
reveal_type(isinstance(RejectingChild(), RejectingBase))  # revealed: Literal[True]

def custom_instancecheck_targets_are_not_exhaustive(x: RejectingChild) -> bool:
    if isinstance(x, (RejectingBase, bytes)):
        return True
    return ""  # error: [invalid-return-type]

def custom_instancecheck_before_target_is_not_exhaustive(x: A) -> bool:  # error: [invalid-return-type]
    if isinstance(x, (RejectingBase, A)):
        return True

T = TypeVar("T")
T_bound_A = TypeVar("T_bound_A", bound=A)
T_constrained = TypeVar("T_constrained", SubclassOfA, OtherSubclassOfA)
T_bound_type = TypeVar("T_bound_type", bound=type)

def bare_type_is_not_exhaustive(x: type) -> bool:  # error: [invalid-return-type]
    if isinstance(x, (type,)):
        return True

bare_type_is_not_exhaustive(list[int])

def indirect_bare_type_is_not_exhaustive(x: T_bound_type) -> bool:  # error: [invalid-return-type]
    if isinstance(x, (type,)):
        return True

indirect_bare_type_is_not_exhaustive(list[int])

def narrowed_bare_type_is_not_exhaustive(x: type) -> bool:  # error: [invalid-return-type]
    if isinstance(x, A):
        return True
    if isinstance(x, (type,)):
        return True

def _(
    x: T,
    x_bound_a: T_bound_A,
    x_constrained_sub_a: T_constrained,
):
    reveal_type(isinstance(x, object))  # revealed: Literal[True]
    reveal_type(isinstance(x, A))  # revealed: bool

    reveal_type(isinstance(x_bound_a, object))  # revealed: Literal[True]
    reveal_type(isinstance(x_bound_a, A))  # revealed: Literal[True]
    reveal_type(isinstance(x_bound_a, SubclassOfA))  # revealed: bool
    reveal_type(isinstance(x_bound_a, B))  # revealed: bool

    reveal_type(isinstance(x_constrained_sub_a, object))  # revealed: Literal[True]
    reveal_type(isinstance(x_constrained_sub_a, A))  # revealed: Literal[True]
    reveal_type(isinstance(x_constrained_sub_a, SubclassOfA))  # revealed: bool
    reveal_type(isinstance(x_constrained_sub_a, OtherSubclassOfA))  # revealed: bool
    reveal_type(isinstance(x_constrained_sub_a, B))  # revealed: bool
```

Certain special forms in the typing module are not instances of `type`, so are strictly-speaking
disallowed as the second argument to `isinstance()` according to typeshed's annotations. However, at
runtime they work fine as the second argument, and we implement that special case in ty:

```py
import typing as t

# no errors emitted for any of these:
isinstance("", t.Dict)
isinstance("", t.List)
isinstance("", t.Set)
isinstance("", t.FrozenSet)
isinstance("", t.Tuple)
isinstance("", t.ChainMap)
isinstance("", t.Counter)
isinstance("", t.Deque)
isinstance("", t.OrderedDict)
isinstance("", t.Callable)
isinstance("", t.Type)
isinstance("", t.Callable | t.Deque)

# `Any` is valid in `issubclass()` calls but not `isinstance()` calls
issubclass(list, t.Any)
issubclass(list, t.Any | t.Dict)

# The same works in tuples
isinstance("", (int, t.Dict))
isinstance("", (int, t.Callable))
issubclass(list, (int, t.Any))
```

But for other special forms that are not permitted as the second argument, we still emit an error:

```py
isinstance("", t.TypeGuard)  # error: [invalid-argument-type]
isinstance("", t.ClassVar)  # error: [invalid-argument-type]
isinstance("", t.Final)  # error: [invalid-argument-type]
isinstance("", t.Any)  # error: [invalid-argument-type]

# The same applies when `Any` is nested inside a tuple
isinstance("", (int, t.Any))  # error: [invalid-argument-type]
```

## Generic builtins should not overfit upper-bound-only callback constraints

These examples are minimized from ecosystem regressions seen while preserving explicit `Never` and
`object` bounds through the constraint solver. The current solver picks callback parameter upper
bounds as concrete solutions when the iterable argument is otherwise unknown. That overfits the
result to `Sized` or `object`; ideally the element type would remain `Unknown`, while the callable
return type would still be used where possible.

```py
from ty_extensions import Unknown

def _(xs: Unknown):
    # TODO: should be `list[Unknown]`
    reveal_type(sorted(xs, key=len))  # revealed: list[Sized]

    # TODO: should be `map[str]`
    reveal_type(map("{}".format, xs))  # revealed: map[object]

    # TODO: should not emit an error and should reveal `str`
    # error: [no-matching-overload]
    reveal_type("".join(map("{}".format, xs)))  # revealed: Unknown
```

## Mapping methods accept arbitrary object types

```toml
[environment]
python-version = "3.13"
```

```py
from collections.abc import Mapping

def _(mapping: Mapping[str, int], dictionary: dict[str, int], key: object) -> None:
    mapping.get(key)
    mapping.get(key, 0)
    mapping.get(key, "default")
    dictionary.get(key)
    dictionary.get(key, 0)
    dictionary.get(key, "default")
    dictionary.pop(key)  # error: [invalid-argument-type]
    dictionary.pop(key, 0)
    dictionary.pop(key, None)
    dictionary.keys().isdisjoint(key)
    dictionary.items().isdisjoint(key)
```

## The builtin `NotImplemented` constant is not callable

```py
def _():
    # snapshot: call-non-callable
    raise NotImplemented()
```

```snapshot
error[call-non-callable]: `NotImplemented` is not callable
 --> src/mdtest_snippet.py:3:11
  |
3 |     raise NotImplemented()
  |           --------------^^
  |           |
  |           Did you mean `NotImplementedError`?
  |
```

```py
def _():
    # snapshot: call-non-callable
    raise NotImplemented("this module is not implemented yet!!!")
```

```snapshot
error[call-non-callable]: `NotImplemented` is not callable
 --> src/mdtest_snippet.py:6:11
  |
6 |     raise NotImplemented("this module is not implemented yet!!!")
  |           --------------^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |           |
  |           Did you mean `NotImplementedError`?
  |
```

## `map` with generic callbacks

```py
from ty_extensions import Unknown
import re

def _(s: Unknown | str):
    escaped = map(re.escape, s)
    reveal_type(escaped)  # revealed: map[str]
    "".join(escaped)

def _(xs: Unknown | list[str]):
    escaped = map(re.escape, xs)
    reveal_type(escaped)  # revealed: map[str]
    tokens: list[Unknown | str] = []
    tokens.extend(escaped)
```
