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

# snapshot: call-non-callable
StillAbstract()

class AbstractBase2(abc.ABC):
    @abc.abstractmethod
    def bar(self): ...
    @abc.abstractmethod
    def bar2(self): ...

# snapshot: call-non-callable
AbstractBase2()

class StillAbstract2(AbstractBase2): ...

# error: [call-non-callable]
StillAbstract2()

class AbstractBase3(Protocol):
    def bar(self) -> None: ...

class StillAbstract3(AbstractBase3): ...

# snapshot: call-non-callable
StillAbstract3()
```

```snapshot
error[call-non-callable]: Cannot instantiate abstract class `StillAbstract`
  --> src/mdtest_snippet.py:11:1
   |
11 |   StillAbstract()
   |   ^^^^^^^^^^^^^^^ `bar` is unimplemented
   |
  ::: src/mdtest_snippet.py:5:5
   |
 5 | /     @abc.abstractmethod
 6 | |     def bar(self): ...
   | |______________________- `bar` declared as abstract on superclass `AbstractBase`


error[call-non-callable]: Cannot instantiate abstract class `AbstractBase2`
  --> src/mdtest_snippet.py:20:1
   |
20 |   AbstractBase2()
   |   ^^^^^^^^^^^^^^^ Abstract methods `bar` and `bar2` are unimplemented
   |
  ::: src/mdtest_snippet.py:14:5
   |
14 | /     @abc.abstractmethod
15 | |     def bar(self): ...
   | |______________________- `bar` declared as abstract


error[call-non-callable]: Cannot instantiate abstract class `StillAbstract3`
  --> src/mdtest_snippet.py:33:1
   |
33 | StillAbstract3()
   | ^^^^^^^^^^^^^^^^ `bar` is unimplemented
   |
  ::: src/mdtest_snippet.py:28:5
   |
28 |     def bar(self) -> None: ...
   |     -------------------------- `bar` declared as abstract on superclass `AbstractBase3`
info: `AbstractBase3.bar` is implicitly abstract because `AbstractBase3` is a `Protocol` class and `bar` lacks an implementation
  --> src/mdtest_snippet.py:27:7
   |
27 | class AbstractBase3(Protocol):
   |       ----------------------- `AbstractBase3` declared here
help: Change the body of `bar` to `return` or `return None` if it was not intended to be abstract
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
class AlsoConcreteOrdered(AbstractOrdered):
    def __gt__(self, other): ...

# total_ordering does not override a comparison method
# if it already exists in the MRO, even if the one that
# exists in the MRO is abstract!
#
# error: [call-non-callable]
AlsoConcreteOrdered()
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
    g: Callable[..., str]

# snapshot: call-non-callable
StillAbstractDynamic()
```

```snapshot
error[call-non-callable]: Cannot instantiate abstract class `StillAbstractDynamic`
  --> src/mdtest_snippet.py:76:1
   |
76 | StillAbstractDynamic()
   | ^^^^^^^^^^^^^^^^^^^^^^ Abstract methods `f` and `g` are unimplemented
   |
  ::: src/mdtest_snippet.py:62:9
   |
62 |     def f(self) -> int: ...
   |         - `f` declared as abstract on superclass `AbstractDynamic`
info: The instance-attribute annotation for `f` does not override the abstract method
help: Either assign a value or add `ClassVar` to this declaration
  --> src/mdtest_snippet.py:72:5
   |
72 |     f: int
   |     - Instance-attribute declaration
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

When a class has many unimplemented abstract methods, the diagnostic lists only a few unless
`--verbose` is enabled.

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

# snapshot: call-non-callable
StillSadlyAbstract()
```

```snapshot
error[call-non-callable]: Cannot instantiate abstract class `StillSadlyAbstract`
   --> src/mdtest_snippet.py:106:1
    |
106 | StillSadlyAbstract()
    | ^^^^^^^^^^^^^^^^^^^^ 10 abstract methods are unimplemented, including `aaaaaaaaa`, `bbbbbbbb` and `cccccccc`
    |
   ::: src/mdtest_snippet.py:92:5
    |
 92 |     def aaaaaaaaa(self) -> int: ...
    |     ------------------------------- `aaaaaaaaa` declared as abstract on superclass `Abstract`
info: Use `--verbose` to see all 10 unimplemented abstract methods
info: `Abstract.aaaaaaaaa` is implicitly abstract because `Abstract` is a `Protocol` class and `aaaaaaaaa` lacks an implementation
  --> src/mdtest_snippet.py:91:7
   |
91 | class Abstract(Protocol):
   |       ------------------ `Abstract` declared here
```

## Verbose diagnostics

With `--verbose`, the diagnostic lists every unimplemented abstract method.

```toml
verbose = true
```

```py
from typing import Protocol

class Abstract(Protocol):
    def first(self) -> int: ...
    def second(self) -> int: ...
    def third(self) -> int: ...
    def fourth(self) -> int: ...

class StillAbstract(Abstract): ...

# snapshot: call-non-callable
StillAbstract()
```

```snapshot
error[call-non-callable]: Cannot instantiate abstract class `StillAbstract`
  --> src/mdtest_snippet.py:12:1
   |
12 | StillAbstract()
   | ^^^^^^^^^^^^^^^ Abstract methods `first`, `second`, `third` and `fourth` are unimplemented
   |
  ::: src/mdtest_snippet.py:4:5
   |
 4 |     def first(self) -> int: ...
   |     --------------------------- `first` declared as abstract on superclass `Abstract`
info: `Abstract.first` is implicitly abstract because `Abstract` is a `Protocol` class and `first` lacks an implementation
 --> src/mdtest_snippet.py:3:7
  |
3 | class Abstract(Protocol):
  |       ------------------ `Abstract` declared here
info: rule `call-non-callable` is enabled by default
```

## Abstract methods without `ABCMeta`

The `abstractmethod` decorator requires subclasses to implement a method even if the class does not
use `ABCMeta`. A default implementation does not remove that requirement. Invalid constructor calls
still retain their inferred instance type for error recovery.

```py
from abc import abstractmethod

class Abstract:
    @abstractmethod
    def method(self) -> int:
        return 42

# error: [call-non-callable]
reveal_type(Abstract())  # revealed: Abstract

class Concrete(Abstract):
    def method(self) -> int:
        return super().method()

Concrete()
```

## Generic abstract classes and aliases

Specializing or aliasing an abstract class does not implement its abstract methods. A concrete
subclass can be instantiated with the same type arguments.

```toml
[environment]
python-version = "3.12"
```

```py
from abc import ABC, abstractmethod

class Abstract[T](ABC):
    @abstractmethod
    def method(self) -> T: ...

Abstract[int]()  # error: [call-non-callable]
Alias = Abstract[str]
Alias()  # error: [call-non-callable]

class Concrete[T](Abstract[T]):
    def method(self) -> T:
        raise NotImplementedError

Concrete[int]()
```

## Constructor calls through `type[]`

A `type[Abstract]` parameter can refer to a concrete subclass, so calling it is allowed. We also
continue to accept abstract class objects as arguments; this check concerns direct construction, not
assignability to `type[]`.

```py
from abc import ABC, abstractmethod

class Abstract(ABC):
    @abstractmethod
    def method(self) -> int: ...

class Concrete(Abstract):
    def method(self) -> int:
        return 42

def construct(cls: type[Abstract]) -> Abstract:
    return cls()

construct(Concrete)
construct(Abstract)
```

## Abstract property accessors

A property remains abstract if its setter or deleter is abstract, even when its getter has a
concrete implementation. Replacing the abstract accessor makes the subclass concrete.

```py
from abc import ABC, abstractmethod

class AbstractSetter(ABC):
    @property
    def value(self) -> int:
        return 0

    @value.setter
    @abstractmethod
    def value(self, value: int) -> None: ...

AbstractSetter()  # error: [call-non-callable]

class ConcreteSetter(AbstractSetter):
    @AbstractSetter.value.setter
    def value(self, value: int) -> None: ...

ConcreteSetter()

class AbstractDeleter(ABC):
    @property
    def value(self) -> int:
        return 0

    @value.deleter
    @abstractmethod
    def value(self) -> None: ...

AbstractDeleter()  # error: [call-non-callable]

class ConcreteDeleter(AbstractDeleter):
    @AbstractDeleter.value.deleter
    def value(self) -> None: ...

ConcreteDeleter()
```

## Methods in stub files

Protocol methods in stub files are abstract only when explicitly decorated with `abstractmethod`. An
empty body can also describe a concrete default implementation whose body is omitted from the stub.

`interface.pyi`:

```pyi
from abc import abstractmethod
from typing import Protocol

class Interface(Protocol):
    def default(self) -> None: ...
    @abstractmethod
    def required(self) -> int: ...
```

`main.py`:

```py
from interface import Interface

class StillAbstract(Interface): ...

StillAbstract()  # error: [call-non-callable] "unimplemented abstract method `required`"

class Concrete(Interface):
    def required(self) -> int:
        return 42

Concrete()
```
