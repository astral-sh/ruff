# Generic callables: Legacy syntax

## Callables can be generic

Many items that are callable can also be generic. Generic functions are the most obvious example:

```py
from typing import TypeVar
from ty_extensions import generic_context

T = TypeVar("T")

def identity(t: T) -> T:
    return t

# revealed: ty_extensions.GenericContext[T@identity]
reveal_type(generic_context(identity))
```

Generic classes are another example, since you invoke the class to instantiate it:

```py
from typing import Generic

class C(Generic[T]):
    def __init__(self, t: T) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
```

When we coerce a generic callable into a `Callable` type, it remembers that it is generic:

```py
from ty_extensions import into_callable

# revealed: [T](t: T) -> T
reveal_type(into_callable(identity))
# revealed: ty_extensions.GenericContext[T@identity]
reveal_type(generic_context(into_callable(identity)))

# revealed: [T](t: T) -> C[T]
reveal_type(into_callable(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_callable(C)))
```

## Naming a generic `Callable`: type aliases

The easiest way to refer to a generic `Callable` type directly is via a type alias:

```py
from typing import Callable, TypeVar
from ty_extensions import generic_context

T = TypeVar("T")

IdentityCallable = Callable[[T], T]

def decorator_factory() -> IdentityCallable[T]:
    def decorator(fn: T) -> T:
        return fn
    # TODO: revealed: ty_extensions.GenericContext[T@decorator]
    # revealed: None
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# TODO: revealed: None
# revealed: ty_extensions.GenericContext[T@decorator_factory]
reveal_type(generic_context(decorator_factory))

# TODO: revealed: [T](T, /) -> T
# revealed: (Unknown, /) -> Unknown
reveal_type(decorator_factory())
# TODO: revealed: ty_extensions.GenericContext[T@IdentityCallable]
# revealed: None
reveal_type(generic_context(decorator_factory()))
```

## Naming a generic `Callable`: function return values

You can also return a generic `Callable` from a function. If a typevar _only_ appears inside of
`Callable`, and _only_ in return type position, then we treat the callable as generic, not the
function, just like above.

```py
from typing import Callable, TypeVar
from ty_extensions import generic_context

T = TypeVar("T")

def decorator_factory() -> Callable[[T], T]:
    def decorator(fn: T) -> T:
        return fn
    # TODO: revealed: ty_extensions.GenericContext[T@decorator]
    # revealed: None
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# TODO: revealed: None
# revealed: ty_extensions.GenericContext[T@decorator_factory]
reveal_type(generic_context(decorator_factory))

# TODO: revealed: [T](T, /) -> T
# revealed: (Unknown, /) -> Unknown
reveal_type(decorator_factory())
# TODO: revealed: ty_extensions.GenericContext[T@bind_context_tbd]
# revealed: None
reveal_type(generic_context(decorator_factory()))
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable(t: T) -> Callable[[T], T]:
    raise NotImplementedError

# revealed: ty_extensions.GenericContext[T@outside_callable]
reveal_type(generic_context(outside_callable))

# revealed: (Literal[1], /) -> Literal[1]
reveal_type(outside_callable(1))
# revealed: None
reveal_type(generic_context(outside_callable(1)))
```
