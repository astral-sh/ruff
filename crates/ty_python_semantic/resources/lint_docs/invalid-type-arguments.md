## What it does
Checks for invalid type arguments in explicit type specialization.

## Why is this bad?
Providing the wrong number of type arguments or type arguments that don't
satisfy the type variable's bounds or constraints will lead to incorrect
type inference and may indicate a misunderstanding of the generic type's
interface.

## Examples

Using legacy type variables:
```python
from typing import Generic, TypeVar

T1 = TypeVar('T1', int, str)
T2 = TypeVar('T2', bound=int)

class Foo1(Generic[T1]): ...
class Foo2(Generic[T2]): ...

Foo1[bytes]  # error: bytes does not satisfy T1's constraints
Foo2[str]  # error: str does not satisfy T2's bound
```

Using PEP 695 type variables:
```python
class Foo[T]: ...
class Bar[T, U]: ...

Foo[int, str]  # error: too many arguments
Bar[int]  # error: too few arguments
```
