# Narrowing For Truthiness Checks (`if x` or `if not x`)

## Value Literals

```py
from typing import Literal

def foo() -> Literal[0, -1, True, False, "", "foo", b"", b"bar", None] | tuple[()]:
    return 0

x = foo()

if x:
    reveal_type(x)  # revealed: Literal[-1, True, "foo", b"bar"]
else:
    reveal_type(x)  # revealed: Literal[0, False, "", b""] | None | tuple[()]

if not x:
    reveal_type(x)  # revealed: Literal[0, False, "", b""] | None | tuple[()]
else:
    reveal_type(x)  # revealed: Literal[-1, True, "foo", b"bar"]

if x and not x:
    reveal_type(x)  # revealed: Never
else:
    reveal_type(x)  # revealed: Literal[0, -1, "", "foo", b"", b"bar"] | bool | None | tuple[()]

if not (x and not x):
    reveal_type(x)  # revealed: Literal[0, -1, "", "foo", b"", b"bar"] | bool | None | tuple[()]
else:
    reveal_type(x)  # revealed: Never

if x or not x:
    reveal_type(x)  # revealed: Literal[0, -1, "", "foo", b"", b"bar"] | bool | None | tuple[()]
else:
    reveal_type(x)  # revealed: Never

if not (x or not x):
    reveal_type(x)  # revealed: Never
else:
    reveal_type(x)  # revealed: Literal[0, -1, "", "foo", b"", b"bar"] | bool | None | tuple[()]

if (isinstance(x, int) or isinstance(x, str)) and x:
    reveal_type(x)  # revealed: Literal[-1, True, "foo"]
else:
    reveal_type(x)  # revealed: Literal[b"", b"bar", 0, False, ""] | None | tuple[()]
```

## Function Literals

Basically functions are always truthy.

```py
def flag() -> bool:
    return True

def foo(hello: int) -> bytes:
    return b""

def bar(world: str, *args, **kwargs) -> float:
    return 0.0

x = foo if flag() else bar

if x:
    reveal_type(x)  # revealed: Literal[foo, bar]
else:
    reveal_type(x)  # revealed: Never
```

## Mutable Truthiness

### Truthiness of Instances

The boolean value of an instance is not always consistent. For example, `__bool__` can be customized
to return random values, or in the case of a `list()`, the result depends on the number of elements
in the list. Therefore, these types should not be narrowed by `if x` or `if not x`.

```py
class A: ...
class B: ...

def f(x: A | B):
    if x:
        reveal_type(x)  # revealed: A & ~AlwaysFalsy | B & ~AlwaysFalsy
    else:
        reveal_type(x)  # revealed: A & ~AlwaysTruthy | B & ~AlwaysTruthy

    if x and not x:
        reveal_type(x)  # revealed: A & ~AlwaysFalsy & ~AlwaysTruthy | B & ~AlwaysFalsy & ~AlwaysTruthy
    else:
        reveal_type(x)  # revealed: A | B

    if x or not x:
        reveal_type(x)  # revealed: A | B
    else:
        reveal_type(x)  # revealed: A & ~AlwaysTruthy & ~AlwaysFalsy | B & ~AlwaysTruthy & ~AlwaysFalsy
```

### Truthiness of Types

Also, types may not be Truthy. This is because `__bool__` can be customized via a metaclass.
Although this is a very rare case, we may consider metaclass checks in the future to handle this
more accurately.

```py
def flag() -> bool:
    return True

x = int if flag() else str
reveal_type(x)  # revealed: Literal[int, str]

if x:
    reveal_type(x)  # revealed: Literal[int] & ~AlwaysFalsy | Literal[str] & ~AlwaysFalsy
else:
    reveal_type(x)  # revealed: Literal[int] & ~AlwaysTruthy | Literal[str] & ~AlwaysTruthy
```

## Determined Truthiness

Some custom classes can have a boolean value that is consistently determined as either `True` or
`False`, regardless of the instance's state. This is achieved by defining a `__bool__` method that
always returns a fixed value.

These types can always be fully narrowed in boolean contexts, as shown below:

```py
from typing import Literal

class T:
    def __bool__(self) -> Literal[True]:
        return True

class F:
    def __bool__(self) -> Literal[False]:
        return False

t = T()

if t:
    reveal_type(t)  # revealed: T
else:
    reveal_type(t)  # revealed: Never

f = F()

if f:
    reveal_type(f)  # revealed: Never
else:
    reveal_type(f)  # revealed: F
```

## Narrowing Complex Intersection and Union

```py
from typing import Literal

class A: ...
class B: ...

def flag() -> bool:
    return True

def instance() -> A | B:
    return A()

def literals() -> Literal[0, 42, "", "hello"]:
    return 42

x = instance()
y = literals()

if isinstance(x, str) and not isinstance(x, B):
    reveal_type(x)  # revealed: A & str & ~B
    reveal_type(y)  # revealed: Literal[0, 42, "", "hello"]

    z = x if flag() else y

    reveal_type(z)  # revealed: A & str & ~B | Literal[0, 42, "", "hello"]

    if z:
        reveal_type(z)  # revealed: A & str & ~B & ~AlwaysFalsy | Literal[42, "hello"]
    else:
        reveal_type(z)  # revealed: A & str & ~B & ~AlwaysTruthy | Literal[0, ""]
```

## Narrowing Multiple Variables

```py
from typing import Literal

def f(x: Literal[0, 1], y: Literal["", "hello"]):
    if x and y and not x and not y:
        reveal_type(x)  # revealed: Never
        reveal_type(y)  # revealed: Never
    else:
        # ~(x or not x) and ~(y or not y)
        reveal_type(x)  # revealed: Literal[0, 1]
        reveal_type(y)  # revealed: Literal["", "hello"]

    if (x or not x) and (y and not y):
        reveal_type(x)  # revealed: Literal[0, 1]
        reveal_type(y)  # revealed: Never
    else:
        # ~(x or not x) or ~(y and not y)
        reveal_type(x)  # revealed: Literal[0, 1]
        reveal_type(y)  # revealed: Literal["", "hello"]
```

## Control Flow Merging

After merging control flows, when we take the union of all constraints applied in each branch, we
should return to the original state.

```py
class A: ...

x = A()

if x and not x:
    y = x
    reveal_type(y)  # revealed: A & ~AlwaysFalsy & ~AlwaysTruthy
else:
    y = x
    reveal_type(y)  # revealed: A

reveal_type(y)  # revealed: A
```

## Truthiness of classes

```py
from typing import Literal

class MetaAmbiguous(type):
    def __bool__(self) -> bool:
        return True

class MetaFalsy(type):
    def __bool__(self) -> Literal[False]:
        return False

class MetaTruthy(type):
    def __bool__(self) -> Literal[True]:
        return True

class MetaDeferred(type):
    def __bool__(self) -> MetaAmbiguous:
        return MetaAmbiguous()

class AmbiguousClass(metaclass=MetaAmbiguous): ...
class FalsyClass(metaclass=MetaFalsy): ...
class TruthyClass(metaclass=MetaTruthy): ...
class DeferredClass(metaclass=MetaDeferred): ...

def _(
    a: type[AmbiguousClass],
    t: type[TruthyClass],
    f: type[FalsyClass],
    d: type[DeferredClass],
    ta: type[TruthyClass | AmbiguousClass],
    af: type[AmbiguousClass] | type[FalsyClass],
    flag: bool,
):
    reveal_type(ta)  # revealed: type[TruthyClass] | type[AmbiguousClass]
    if ta:
        reveal_type(ta)  # revealed: type[TruthyClass] | type[AmbiguousClass] & ~AlwaysFalsy

    reveal_type(af)  # revealed: type[AmbiguousClass] | type[FalsyClass]
    if af:
        reveal_type(af)  # revealed: type[AmbiguousClass] & ~AlwaysFalsy

    # error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `MetaDeferred`; the return type of its bool method (`MetaAmbiguous`) isn't assignable to `bool"
    if d:
        # TODO: Should be `Unknown`
        reveal_type(d)  # revealed: type[DeferredClass] & ~AlwaysFalsy

    tf = TruthyClass if flag else FalsyClass
    reveal_type(tf)  # revealed: Literal[TruthyClass, FalsyClass]

    if tf:
        reveal_type(tf)  # revealed: Literal[TruthyClass]
    else:
        reveal_type(tf)  # revealed: Literal[FalsyClass]
```

## Narrowing in chained boolean expressions

```py
from typing import Literal

class A: ...

def _(x: Literal[0, 1]):
    reveal_type(x or A())  # revealed: Literal[1] | A
    reveal_type(x and A())  # revealed: Literal[0] | A

def _(x: str):
    reveal_type(x or A())  # revealed: str & ~AlwaysFalsy | A
    reveal_type(x and A())  # revealed: str & ~AlwaysTruthy | A

def _(x: bool | str):
    reveal_type(x or A())  # revealed: Literal[True] | str & ~AlwaysFalsy | A
    reveal_type(x and A())  # revealed: Literal[False] | str & ~AlwaysTruthy | A

class Falsy:
    def __bool__(self) -> Literal[False]:
        return False

class Truthy:
    def __bool__(self) -> Literal[True]:
        return True

def _(x: Falsy | Truthy):
    reveal_type(x or A())  # revealed: Truthy | A
    reveal_type(x and A())  # revealed: Falsy | A

class MetaFalsy(type):
    def __bool__(self) -> Literal[False]:
        return False

class MetaTruthy(type):
    def __bool__(self) -> Literal[True]:
        return True

class FalsyClass(metaclass=MetaFalsy): ...
class TruthyClass(metaclass=MetaTruthy): ...

def _(x: type[FalsyClass] | type[TruthyClass]):
    reveal_type(x or A())  # revealed: type[TruthyClass] | A
    reveal_type(x and A())  # revealed: type[FalsyClass] | A
```

## Truthiness narrowing for `LiteralString`

```py
from typing_extensions import LiteralString

def _(x: LiteralString):
    if x:
        reveal_type(x)  # revealed: LiteralString & ~Literal[""]
    else:
        reveal_type(x)  # revealed: Literal[""]

    if not x:
        reveal_type(x)  # revealed: Literal[""]
    else:
        reveal_type(x)  # revealed: LiteralString & ~Literal[""]
```
