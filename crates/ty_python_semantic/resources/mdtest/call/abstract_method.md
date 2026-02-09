# Calling abstract methods on class objects

## Abstract classmethod with trivial body on class literal

Calling an abstract `@classmethod` with a trivial body directly on the class is unsound.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int: ...

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
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

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract staticmethod with a trivial body"
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

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
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

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
Foo.method()
```

## Recursive abstract classmethod body does not recurse

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int:
        # error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
        raise NotImplementedError(Foo.method())

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
Foo.method()
```

## Recursive abstract staticmethod body does not recurse

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @staticmethod
    @abstractmethod
    def method() -> int:
        # error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract staticmethod with a trivial body"
        raise NotImplementedError(Foo.method())

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract staticmethod with a trivial body"
Foo.method()
```

## Non-abstract classmethod is fine

```py
from abc import ABC

class Foo(ABC):
    @classmethod
    def method(cls) -> int:
        return 42

Foo.method()  # fine
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

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
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

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
Foo.method()
```

## Abstract staticmethod with non-trivial body

An abstract staticmethod with a non-trivial body has a default implementation that can be called.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @staticmethod
    @abstractmethod
    def method() -> int:
        return 42

Foo.method()  # fine
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

# error: [call-abstract-method] "Cannot call method `method` on class object because it is an abstract classmethod with a trivial body"
Foo.method()
```

## Abstract instance method is not caught

This lint only applies to classmethods and staticmethods, not regular instance methods.

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @abstractmethod
    def method(self) -> int: ...

# This would be caught by other mechanisms (Foo is abstract and cannot be instantiated),
# but this lint specifically does not apply here.
```
