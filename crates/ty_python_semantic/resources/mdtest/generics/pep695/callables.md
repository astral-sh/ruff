# Generic callables: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

## Callables can be generic

Many items that are callable can also be generic. Generic functions are the most obvious example:

```py
from ty_extensions import generic_context

def identity[T](t: T) -> T:
    return t

# revealed: ty_extensions.GenericContext[T@identity]
reveal_type(generic_context(identity))
```

Generic classes are another example, since you invoke the class to instantiate it:

```py
class C[T]:
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
from typing import Callable
from ty_extensions import generic_context

type IdentityCallable[T] = Callable[[T], T]

def decorator_factory[T]() -> IdentityCallable[T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions.GenericContext[T@decorator]
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

NOTE: This is one place where the PEP-695 syntax is misleading! It _looks_ like `decorator_factory`
is generic, since it contains a `[T]` binding context. However, we still notice that the only _use_
of `T` in the signature is in the return type, inside of a `Callable` — and so it is the returned
callable that is generic, not the function.

```py
from typing import Callable
from ty_extensions import generic_context

def decorator_factory[T]() -> Callable[[T], T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T](T, /) -> T
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[T@decorator_factory]
reveal_type(generic_context(decorator_factory()))
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable[T](t: T) -> Callable[[T], T]:
    raise NotImplementedError

# revealed: ty_extensions.GenericContext[T@outside_callable]
reveal_type(generic_context(outside_callable))

# revealed: (Literal[1], /) -> Literal[1]
reveal_type(outside_callable(1))
# revealed: None
reveal_type(generic_context(outside_callable(1)))
```
