# Calling abstract methods on class objects

## Abstract classmethod with trivial body on class literal

<!-- snapshot-diagnostics -->

Calling an abstract `@classmethod` with a trivial body directly on the class is unsound.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int: ...

# error: [call-abstract-method] "Cannot call `method` on class object"
Foo.method()
```

## Abstract staticmethod with trivial body on class literal

Calling an abstract `@staticmethod` with a trivial body directly on the class is unsound.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @staticmethod
    @abstractmethod
    def method() -> int: ...

# error: [call-abstract-method] "Cannot call `method` on class object"
Foo.method()
```

## Abstract classmethod via `type[X]` is allowed

When accessed via `type[X]`, the runtime type could be a concrete subclass.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int: ...

def f(x: type[Foo]):
    x.method()  # fine
```

## Abstract staticmethod via `type[X]` is allowed

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @staticmethod
    @abstractmethod
    def method() -> int: ...

def f(x: type[Foo]):
    x.method()  # fine
```

## Abstract classmethod with non-trivial body

An abstract method with a non-trivial body has a default implementation that can be called.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int:
        return 42

Foo.method()  # fine
```

## Abstract classmethod with `pass` body

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int:
        pass

# error: [call-abstract-method] "Cannot call `method` on class object"
Foo.method()
```

## Abstract classmethod with `raise NotImplementedError` body

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int:
        raise NotImplementedError

# error: [call-abstract-method] "Cannot call `method` on class object"
Foo.method()
```

## Concrete subclass calling inherited abstract classmethod

```py
from abc import ABC, abstractmethod

class Base(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int: ...

class Derived(Base):
    @classmethod
    def method(cls) -> int:
        return 42

Derived.method()  # fine
```

## Abstract subclass that does not override the abstract classmethod

If a subclass is still abstract (doesn't override the method), calling the inherited abstract
classmethod on it is also unsound.

```py
from abc import ABC, abstractmethod

class Base(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int: ...

class StillAbstract(Base):
    pass

# error: [call-abstract-method] "Cannot call `method` on class object"
StillAbstract.method()
```

## Reversed decorator order

The `@abstractmethod` and `@classmethod`/`@staticmethod` decorators can appear in either order.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @abstractmethod
    @classmethod
    def method(cls) -> int: ...

# error: [call-abstract-method] "Cannot call `method` on class object"
Foo.method()
```

## Abstract classmethod with docstring-only body

A method whose body is only a docstring is also a trivial body.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int:
        """This method should be overridden."""
        ...

# error: [call-abstract-method] "Cannot call `method` on class object"
Foo.method()
```

## Implicitly abstract classmethod on Protocol

A classmethod on a `Protocol` class with a trivial body is implicitly abstract, even without
`@abstractmethod`.

```py
from typing import Protocol

class P(Protocol):
    @classmethod
    def method(cls) -> int: ...

# error: [call-abstract-method] "Cannot call `method` on class object"
P.method()
```

## Implicitly abstract staticmethod on Protocol

```py
from typing import Protocol

class P(Protocol):
    @staticmethod
    def method() -> int: ...

# error: [call-abstract-method] "Cannot call `method` on class object"
P.method()
```

## Abstract classmethod defined in stub file

Methods defined in stub files are never considered to have trivial bodies, since stubs use `...` as
a placeholder regardless of the runtime implementation.

`foo.pyi`:

```pyi
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def classmethod(cls) -> int: ...
    @staticmethod
    @abstractmethod
    def staticmethod() -> int: ...
```

```py
from foo import Foo

# These should not trigger `call-abstract-method` since the methods are defined in a stub.
Foo.classmethod()
Foo.staticmethod()
```
