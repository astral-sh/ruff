# Empty body function tests

## Empty body with non-None return type

Functions with empty bodies and non-`None` return types should trigger the `empty-body` error.

### Basic empty body cases

```py
# error: [empty-body]
def foo() -> int: ...

# error: [empty-body]
def bar() -> str:
    pass

# error: [empty-body]
def baz() -> list[int]:
    """A function that does nothing."""

# error: [empty-body]
def qux() -> dict[str, int]:
    """Docstring"""
    ...

# error: [empty-body]
def quux() -> bool:
    """Docstring"""
    pass
```

### Valid empty body cases (should NOT trigger empty-body)

Functions returning `None` are valid:

```py
def returns_none() -> None: ...

def also_returns_none() -> None:
    pass

def explicit_none() -> None:
    """Does nothing and returns None"""
```

Functions with actual implementations are valid:

```py
def has_implementation() -> int:
    return 42

def has_body_with_pass() -> int:
    x = 1
    pass
    return x

def raises() -> int:
    raise NotImplementedError()
```

## Allowed contexts for empty bodies

### In Protocol classes

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol

class MyProtocol(Protocol):
    def method(self) -> int: ...
    
    def another_method(self) -> str:
        """Docstring"""
        pass

# Non-Protocol class should still error
class NotAProtocol:
    # error: [empty-body]
    def method(self) -> int: ...
```

### With abstractmethod decorator

```py
from abc import ABC, abstractmethod

class Base(ABC):
    @abstractmethod
    def abstract_method(self) -> int: ...
    
    @abstractmethod
    def another_abstract(self) -> str:
        pass

# Without abstractmethod, should error
class Derived(ABC):
    # error: [empty-body]
    def not_abstract(self) -> int: ...
```

### With overload decorator

```py
from typing import overload

@overload
def overloaded(x: int) -> int: ...

@overload
def overloaded(x: str) -> str: ...

def overloaded(x: int | str) -> int | str:
    return x

# Without @overload, should error
# error: [empty-body]
def not_overload() -> int: ...
```

### In TYPE_CHECKING blocks

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    def type_checking_func() -> int: ...
    
    def another_type_checking() -> str:
        """Only for type checking"""

# Outside TYPE_CHECKING, should error
# error: [empty-body]
def not_in_type_checking() -> int: ...
```

## Mixed cases

### Class methods

```py
class MyClass:
    # error: [empty-body]
    def method(self) -> int: ...
    
    # error: [empty-body]
    def class_method(cls) -> str:
        pass
    
    def valid_method(self) -> int:
        return 42
```

### Generic functions

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypeVar

T = TypeVar('T')

# error: [empty-body]
def generic_func[T](x: T) -> T: ...

# Valid: has implementation
def valid_generic[T](x: T) -> T:
    return x
```

### Async functions

```py
# error: [empty-body]
async def async_func() -> int: ...

# error: [empty-body]
async def async_with_pass() -> str:
    pass

# Valid: has implementation
async def valid_async() -> int:
    return 42
```

## Edge cases

### Nested functions

An outer function with an empty body should trigger empty-body even if it contains
inner functions with implementations.

```py
def outer() -> int:  # error: [invalid-return-type]
    def inner() -> str:
        return "hello"
```

### Decorators (non-special)

Regular decorators (not @abstractmethod, @overload, etc.) don't make empty bodies valid.

```py
def my_decorator(f):
    return f

@my_decorator
def decorated() -> int:  # error: [empty-body]
    ...
```
