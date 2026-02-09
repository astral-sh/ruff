# Generic callables: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

## Callables can be generic

Many items that are callable can also be generic. Generic functions are the most obvious example:

```py
from typing import Callable
from ty_extensions import generic_context

def identity[T](t: T) -> T:
    return t

# revealed: ty_extensions.GenericContext[T@identity]
reveal_type(generic_context(identity))
# revealed: Literal[1]
reveal_type(identity(1))

def identity2[**P, T](c: Callable[P, T]) -> Callable[P, T]:
    return c

# revealed: ty_extensions.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(identity2))
# revealed: [T](t: T) -> T
reveal_type(identity2(identity))
```

Generic classes are another example, since you invoke the class to instantiate it:

```py
class C[T]:
    t: T  # invariant

    def __init__(self, t: T) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: C[int]
reveal_type(C(1))
```

When we coerce a generic callable into a `Callable` type, it remembers that it is generic:

```py
from ty_extensions import into_callable

# revealed: [T](t: T) -> T
reveal_type(into_callable(identity))
# revealed: ty_extensions.GenericContext[T@identity]
reveal_type(generic_context(into_callable(identity)))
# revealed: Literal[1]
reveal_type(into_callable(identity)(1))

# revealed: [**P, T](c: (**P@identity2) -> T) -> (**P@identity2) -> T
reveal_type(into_callable(identity2))
# revealed: ty_extensions.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(into_callable(identity2)))
# revealed: [T](t: T) -> T
reveal_type(into_callable(identity2)(identity))

# revealed: [T](t: T) -> C[T]
reveal_type(into_callable(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_callable(C)))
# revealed: C[int]
reveal_type(into_callable(C)(1))
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
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
```

## Naming a generic `Callable` with paramspecs: type aliases

The same pattern holds if the callable involves a paramspec.

```py
from typing import Callable
from ty_extensions import generic_context

type IdentityCallable[**P, T] = Callable[[Callable[P, T]], Callable[P, T]]

def decorator_factory[**P, T]() -> IdentityCallable[P, T]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return@decorator_factory) -> T'return, /) -> (**P'return@decorator_factory) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: [T](t: T) -> T
reveal_type(decorator_factory()(identity))
# revealed: Literal[1]
reveal_type(decorator_factory()(identity)(1))
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

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
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
# error: [invalid-argument-type]
outside_callable(1)("string")
```

## Naming a generic `Callable` with paramspecs: function return values

The same pattern holds if the callable involves a paramspec.

```py
from typing import Callable
from ty_extensions import generic_context

def decorator_factory[**P, T]() -> Callable[[Callable[P, T]], Callable[P, T]]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return@decorator_factory) -> T'return, /) -> (**P'return@decorator_factory) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: [T](t: T) -> T
reveal_type(decorator_factory()(identity))
# revealed: Literal[1]
reveal_type(decorator_factory()(identity)(1))
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable[**P, T](func: Callable[P, T]) -> Callable[P, T]:
    raise NotImplementedError

# revealed: ty_extensions.GenericContext[P@outside_callable, T@outside_callable]
reveal_type(generic_context(outside_callable))

def int_identity(x: int) -> int:
    return x

# revealed: (x: int) -> int
reveal_type(outside_callable(int_identity))
# revealed: None
reveal_type(generic_context(outside_callable(int_identity)))
# error: [invalid-argument-type]
outside_callable(int_identity)("string")
```
