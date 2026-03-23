# Tests regarding abstract classes

## Instantiation is forbidden

Classes with unimplemented abstract methods cannot be instantiated. Type checkers are expected to
detect possible attempts to instantiate abstract classes:

```py
import abc
from typing import Protocol

class AbstractBase(abc.ABC):
    @abc.abstractmethod
    def bar(self): ...

class StillAbstract(AbstractBase): ...

# TODO: should emit diagnostic
StillAbstract()

class AbstractBase2(abc.ABC):
    @abc.abstractmethod
    def bar(self): ...
    @abc.abstractmethod
    def bar2(self): ...

# TODO: should emit diagnostic
AbstractBase2()

class StillAbstract2(AbstractBase2): ...

# TODO: should emit diagnostic
StillAbstract2()

class AbstractBase3(Protocol):
    def bar(self) -> int: ...

class StillAbstract3(AbstractBase3): ...

# TODO: should emit diagnostic
StillAbstract3()
```

Abstract methods can be concretely overridden by synthesized methods:

```py
from abc import ABC, abstractmethod
from dataclasses import dataclass
from functools import total_ordering

class AbstractOrdered(ABC):
    @abstractmethod
    def __lt__(self, other): ...

@dataclass(order=True)
class ConcreteOrdered(AbstractOrdered): ...

ConcreteOrdered()  # fine

@total_ordering
class AlsoConreteOrdered(AbstractOrdered):
    def __gt__(self, other): ...

# total_ordering does not override a comparison method
# if it already exists in the MRO, even if the one that
# exists in the MRO is abstract!
#
# TODO: should emit diagnostic
AlsoConreteOrdered()
```

We also allow abstract methods or properties to be "overridden" by a `ClassVar` annotation, even if
it is not accompanied by a binding in the class body: we assume that a class like this will have the
override added dynamically (e.g., by a metaclass):

```py
from typing import ClassVar, Callable

class AbstractDynamic(ABC):
    @property
    @abstractmethod
    def f(self) -> int: ...
    @abstractmethod
    def g(self) -> str: ...

class ConcreteDynamic(AbstractDynamic):
    f: ClassVar[int]
    g: ClassVar[Callable[..., str]]

ConcreteDynamic()  # no error
```

But if the annotation does not use `ClassVar`, we do not see that as overriding the abstract method:

```py
class StillAbstractDynamic(AbstractDynamic):
    f: int

StillAbstractDynamic()  # TODO: should emit an error!
```

Abstract methods can be overridden by mixin classes, but the concrete override on the mixin must
come earlier in the MRO:

```py
class AbstractMixin(ABC):
    @abstractmethod
    def bar(self): ...

class ConcreteMixin:
    def bar(self): ...

class Sub1(AbstractMixin, ConcreteMixin): ...
class Sub2(ConcreteMixin, AbstractMixin): ...

Sub1()  # TODO: should emit diagnostic
Sub2()  # fine
```

A test for our diagnostic when a class has many unimplemented abstract methods:

```py
from typing import Protocol

class Abstract(Protocol):
    def aaaaaaaaa(self) -> int: ...
    def bbbbbbbb(self) -> int: ...
    def cccccccc(self) -> int: ...
    def dddddddddd(self) -> int: ...
    def eeeeeeee(self) -> int: ...
    def fffffff(self) -> int: ...
    def ggggggggg(self) -> int: ...
    def hhhhhhhhh(self) -> int: ...
    def iiiiiiiiii(self) -> int: ...
    def kkkkkkkkk(self) -> int: ...

class StillSadlyAbstract(Abstract): ...

StillSadlyAbstract()  # TODO: should emit diagnostic
```
