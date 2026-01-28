# Tests regarding abstract classes

## Instantiation is forbidden

<!-- snapshot-diagnostics -->

Classes with unimplemented abstract methods cannot be instantiated. Type checkers are expected to
detect possible attempts to instantiate abstract classes:

```py
import abc
from typing import Protocol

class Foo(abc.ABC):
    @abc.abstractmethod
    def bar(self): ...

class Spam(Foo): ...

# error: [call-non-callable] "Cannot instantiate `Spam` with unimplemented abstract method `bar`"
Spam()

class Foo2(abc.ABC):
    @abc.abstractmethod
    def bar(self): ...
    @abc.abstractmethod
    def bar2(self): ...

# error: [call-non-callable] "Cannot instantiate `Foo2` with unimplemented abstract methods `bar` and `bar2`"
Foo2()

class Spam2(Foo2): ...

# error: [call-non-callable]
Spam2()

class Foo3(Protocol):
    def bar(self) -> int: ...

class Spam3(Foo3): ...

# error: [call-non-callable]
Spam3()
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
# error: [call-non-callable]
AlsoConreteOrdered()
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

Sub1()  # error: [call-non-callable]
Sub2()  # fine
```

If the class has many unimplemented abstract methods, we do not list them all the diagnostic unless
the user has specified `--verbose`:

```py
from typing import Protocol

class Abstract(Protocol):
    def a(self) -> int: ...
    def b(self) -> int: ...
    def c(self) -> int: ...
    def d(self) -> int: ...
    def e(self) -> int: ...
    def f(self) -> int: ...
    def g(self) -> int: ...
    def h(self) -> int: ...
    def i(self) -> int: ...
    def k(self) -> int: ...

class StillSadlyAbstract(Abstract): ...

StillSadlyAbstract()  # error: [call-non-callable]
```
