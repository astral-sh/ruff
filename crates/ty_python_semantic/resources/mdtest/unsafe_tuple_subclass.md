# Unsafe Tuple Subclass

We do various kinds of narrowing on tuples and tuple subclasses. For these narrowings to be sound,
we assume that tuple subclasses don't override certain dunder methods.

## `__eq__`

<!-- snapshot-diagnostics -->

```py
class A(tuple):
    # error: [unsafe-tuple-subclass]
    def __eq__(self, other):
        return False

class B(tuple): ...

class C(B):
    # error: [unsafe-tuple-subclass]
    def __eq__(self, other):
        return False

class D(tuple):
    # error: [unsafe-tuple-subclass]
    def __ne__(self, other):
        return False
```


## `__bool__`

<!-- snapshot-diagnostics -->

```py
from typing import Literal

class A(tuple[()]):
    # ok
    def __bool__(self):
        return False

class B(tuple[()]):
    # ok - tuple is always false, returns always false
    def __bool__(self) -> Literal[False]:
        return False

class C(tuple[()]):
    # error: [unsafe-tuple-subclass]
    def __bool__(self) -> Literal[True]:
        return True

class D(tuple[()]):
    # error: [unsafe-tuple-subclass]
    def __bool__(self) -> bool:
        return True

class E(tuple[int, int]):
    # ok
    def __bool__(self):
        return True

class F(tuple[int, int]):
    # error: [unsafe-tuple-subclass]
    def __bool__(self) -> Literal[False]:
        return False

class G(tuple[int, int]):
    # ok - tuple is always true, returns always true
    def __bool__(self) -> Literal[True]:
        return True

class H(tuple[int, int]):
    # error: [unsafe-tuple-subclass]
    def __bool__(self) -> bool:
        return False

class I(tuple[int, ...]):
    # ok - ambiguous tuple, any return is safe
    def __bool__(self):
        return False

class J(tuple[int, ...]):
    # ok - ambiguous tuple, any return is safe
    def __bool__(self) -> Literal[False]:
        return False

class K(tuple[int, ...]):
    # ok - ambiguous tuple, any return is safe
    def __bool__(self) -> Literal[True]:
        return True

class L(tuple[int, ...]):
    # ok - ambiguous tuple, any return is safe
    def __bool__(self) -> bool:
        return True
```
