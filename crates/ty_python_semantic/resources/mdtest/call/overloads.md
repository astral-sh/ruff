# Overloads

When ty evaluates the call of an overloaded function, it attempts to "match" the supplied arguments
with one or more overloads. This document describes the algorithm that it uses for overload
matching, which is the same as the one mentioned in the
[spec](https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation).

## Arity check

The first step is to perform arity check. The non-overloaded cases are described in the
[function](./function.md) document.

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f() -> None: ...
@overload
def f(x: int) -> int: ...
```

```py
from overloaded import f

# These match a single overload
reveal_type(f())  # revealed: None
reveal_type(f(1))  # revealed: int

# error: [no-matching-overload] "No overload of function `f` matches arguments"
reveal_type(f("a", "b"))  # revealed: Unknown
```

## Type checking

The second step is to perform type checking. This is done for all the overloads that passed the
arity check.

### Single match

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
@overload
def f(x: bytes) -> bytes: ...
```

Here, all of the calls below pass the arity check for all overloads, so we proceed to type checking
which filters out all but the matching overload:

```py
from overloaded import f

reveal_type(f(1))  # revealed: int
reveal_type(f("a"))  # revealed: str
reveal_type(f(b"b"))  # revealed: bytes
```

### Single match error

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def f() -> None: ...
@overload
def f(x: int) -> int: ...
```

If the arity check only matches a single overload, it should be evaluated as a regular
(non-overloaded) function call. This means that any diagnostics resulted during type checking that
call should be reported directly and not as a `no-matching-overload` error.

```py
from overloaded import f

reveal_type(f())  # revealed: None

# TODO: This should be `invalid-argument-type` instead
# error: [no-matching-overload]
reveal_type(f("a"))  # revealed: Unknown
```

### Multiple matches

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B(A): ...

@overload
def f(x: A) -> A: ...
@overload
def f(x: B, y: int = 0) -> B: ...
```

```py
from overloaded import A, B, f

# These calls pass the arity check, and type checking matches both overloads:
reveal_type(f(A()))  # revealed: A
reveal_type(f(B()))  # revealed: A

# But, in this case, the arity check filters out the first overload, so we only have one match:
reveal_type(f(B(), 1))  # revealed: B
```

## Argument type expansion

This step is performed only if the previous steps resulted in **no matches**.

In this case, the algorithm would perform
[argument type expansion](https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion)
and loops over from the type checking step, evaluating the argument lists.

### Expanding the only argument

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...

@overload
def f(x: A) -> A: ...
@overload
def f(x: B) -> B: ...
@overload
def f(x: C) -> C: ...
```

```py
from overloaded import A, B, C, f

def _(ab: A | B, ac: A | C, bc: B | C):
    reveal_type(f(ab))  # revealed: A | B
    reveal_type(f(bc))  # revealed: B | C
    reveal_type(f(ac))  # revealed: A | C
```

### Expanding first argument

If the set of argument lists created by expanding the first argument evaluates successfully, the
algorithm shouldn't expand the second argument.

`overloaded.pyi`:

```pyi
from typing import Literal, overload

class A: ...
class B: ...
class C: ...
class D: ...

@overload
def f(x: A, y: C) -> A: ...
@overload
def f(x: A, y: D) -> B: ...
@overload
def f(x: B, y: C) -> C: ...
@overload
def f(x: B, y: D) -> D: ...
```

```py
from overloaded import A, B, C, D, f

def _(a_b: A | B):
    reveal_type(f(a_b, C()))  # revealed: A | C
    reveal_type(f(a_b, D()))  # revealed: B | D

# But, if it doesn't, it should expand the second argument and try again:
def _(a_b: A | B, c_d: C | D):
    reveal_type(f(a_b, c_d))  # revealed: A | B | C | D
```

### Expanding second argument

If the first argument cannot be expanded, the algorithm should move on to the second argument,
keeping the first argument as is.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...
class D: ...

@overload
def f(x: A, y: B) -> B: ...
@overload
def f(x: A, y: C) -> C: ...
@overload
def f(x: B, y: D) -> D: ...
```

```py
from overloaded import A, B, C, D, f

def _(a: A, bc: B | C, cd: C | D):
    # This also tests that partial matching works correctly as the argument type expansion results
    # in matching the first and second overloads, but not the third one.
    reveal_type(f(a, bc))  # revealed: B | C

    # error: [no-matching-overload] "No overload of function `f` matches arguments"
    reveal_type(f(a, cd))  # revealed: Unknown
```

### Generics (legacy)

`overloaded.pyi`:

```pyi
from typing import TypeVar, overload

_T = TypeVar("_T")

class A: ...
class B: ...

@overload
def f(x: A) -> A: ...
@overload
def f(x: _T) -> _T: ...
```

```py
from overloaded import A, f

def _(x: int, y: A | int):
    reveal_type(f(x))  # revealed: int
    reveal_type(f(y))  # revealed: A | int
```

### Generics (PEP 695)

```toml
[environment]
python-version = "3.12"
```

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...

@overload
def f(x: B) -> B: ...
@overload
def f[T](x: T) -> T: ...
```

```py
from overloaded import B, f

def _(x: int, y: B | int):
    reveal_type(f(x))  # revealed: int
    reveal_type(f(y))  # revealed: B | int
```

### Expanding `bool`

`overloaded.pyi`:

```pyi
from typing import Literal, overload

class T: ...
class F: ...

@overload
def f(x: Literal[True]) -> T: ...
@overload
def f(x: Literal[False]) -> F: ...
```

```py
from overloaded import f

def _(flag: bool):
    reveal_type(f(True))  # revealed: T
    reveal_type(f(False))  # revealed: F
    reveal_type(f(flag))  # revealed: T | F
```

### Expanding `tuple`

`overloaded.pyi`:

```pyi
from typing import Literal, overload

class A: ...
class B: ...
class C: ...
class D: ...

@overload
def f(x: tuple[A, int], y: tuple[int, Literal[True]]) -> A: ...
@overload
def f(x: tuple[A, int], y: tuple[int, Literal[False]]) -> B: ...
@overload
def f(x: tuple[B, int], y: tuple[int, Literal[True]]) -> C: ...
@overload
def f(x: tuple[B, int], y: tuple[int, Literal[False]]) -> D: ...
```

```py
from overloaded import A, B, f

def _(x: tuple[A | B, int], y: tuple[int, bool]):
    reveal_type(f(x, y))  # revealed: A | B | C | D
```

### Expanding `type`

There's no special handling for expanding `type[A | B]` type because ty stores this type in it's
distributed form, which is `type[A] | type[B]`.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...

@overload
def f(x: type[A]) -> A: ...
@overload
def f(x: type[B]) -> B: ...
```

```py
from overloaded import A, B, f

def _(x: type[A | B]):
    reveal_type(x)  # revealed: type[A] | type[B]
    reveal_type(f(x))  # revealed: A | B
```

### Expanding enums

`overloaded.pyi`:

```pyi
from enum import Enum
from typing import Literal, overload

class SomeEnum(Enum):
    A = 1
    B = 2
    C = 3


class A: ...
class B: ...
class C: ...

@overload
def f(x: Literal[SomeEnum.A]) -> A: ...
@overload
def f(x: Literal[SomeEnum.B]) -> B: ...
@overload
def f(x: Literal[SomeEnum.C]) -> C: ...
```

```py
from overloaded import SomeEnum, A, B, C, f

def _(x: SomeEnum):
    reveal_type(f(SomeEnum.A))  # revealed: A
    # TODO: This should be `B` once enums are supported and are expanded
    reveal_type(f(SomeEnum.B))  # revealed: A
    # TODO: This should be `C` once enums are supported and are expanded
    reveal_type(f(SomeEnum.C))  # revealed: A
    # TODO: This should be `A | B | C` once enums are supported and are expanded
    reveal_type(f(x))  # revealed: A
```
