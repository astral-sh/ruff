# Unsafe Tuple Subclass

We do various kinds of narrowing on tuples and tuple subclasses. For these narrowings to be sound,
we assume that tuple subclasses don't override certain dunder methods.

## `__eq__` and `__ne__`

We emit diagnostics on any method override of the `__eq__` and `__ne__` methods.

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

We should only emit diagnostics for overrides of `__bool__` if we can be sure that it is invalid.
This means we only emit diagnostics on subclasses of fixed length tuples if the return type of the
`__bool__` method is annotated and it does not match the expected return type of the `__bool__` of
the `tuple` superclass.

<!-- snapshot-diagnostics -->

```py
from typing import Literal

class A(tuple[()]):
    # ok
    def __bool__(self):
        return False

class B(tuple[()]):
    # ok
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
    # ok
    def __bool__(self) -> Literal[True]:
        return True

class H(tuple[int, int]):
    # error: [unsafe-tuple-subclass]
    def __bool__(self) -> bool:
        return False

class I(tuple[int, ...]):
    # ok
    def __bool__(self):
        return False

class J(tuple[int, ...]):
    # ok
    def __bool__(self) -> Literal[False]:
        return False

class K(tuple[int, ...]):
    # ok
    def __bool__(self) -> Literal[True]:
        return True

class L(tuple[int, ...]):
    # ok
    def __bool__(self) -> bool:
        return True
```
