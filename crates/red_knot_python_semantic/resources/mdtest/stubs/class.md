# Class definitions in stubs

## Cyclical class definition

In type stubs, classes can reference themselves in their base class definitions. For example, in
`typeshed`, we have `class str(Sequence[str]): ...`.

```pyi
class Foo[T]: ...

# TODO: actually is subscriptable
# error: [non-subscriptable]
class Bar(Foo[Bar]): ...

reveal_type(Bar)  # revealed: Literal[Bar]
reveal_type(Bar.__mro__)  # revealed: tuple[Literal[Bar], Unknown, Literal[object]]
```

## Acces to attributes declarated in stubs

Unlike regular python modules, stub files often declare module-global and class variables without
initializing them. But from the perspective of the type checker, we should treat something like
`symbol: type` same as `symbol: type = ...`.

`b.pyi`:

```pyi
from typing import ClassVar

class C:
    a_classvar: ClassVar[str]
    instance_var: int
```

```py
from typing import ClassVar, Literal

from b import C

reveal_type(C.a_classvar)  # revealed: str

class Subclass(C):
    unbound_classvar: ClassVar[str]
    declared: int
    declared_and_bound: int = 42

# TODO: this should be an error showing possibly unbound.
reveal_type(Subclass.unbound_classvar)  # revealed: str

s_inst = Subclass()

reveal_type(s_inst.instance_var)  # revealed: int

# TODO: this should be an error showing possibly unbound.
reveal_type(s_inst.declared)  # revealed: int

reveal_type(s_inst.declared_and_bound)  # revealed: int
```
