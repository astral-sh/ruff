# Generic classes

## PEP 695 syntax

TODO: Add a `red_knot_extension` function that asserts whether a function or class is generic.

This is a generic class defined using PEP 695 syntax:

```py
class C[T]: ...
```

A class that inherits from a generic class, and fills its type parameters with typevars, is generic:

```py
# TODO: no error
# error: [non-subscriptable]
class D[U](C[U]): ...
```

A class that inherits from a generic class, but fills its type parameters with concrete types, is
_not_ generic:

```py
# TODO: no error
# error: [non-subscriptable]
class E(C[int]): ...
```

A class that inherits from a generic class, and doesn't fill its type parameters at all, implicitly
uses the default value for the typevar. In this case, that default type is `Unknown`, so `F`
inherits from `C[Unknown]` and is not itself generic.

```py
class F(C): ...
```

## Legacy syntax

This is a generic class defined using the legacy syntax:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

# TODO: no error
# error: [invalid-base]
class C(Generic[T]): ...
```

A class that inherits from a generic class, and fills its type parameters with typevars, is generic.

```py
class D(C[T]): ...
```

(Examples `E` and `F` from above do not have analogues in the legacy syntax.)

## Inferring generic class parameters

The type parameter can be specified explicitly:

```py
class C[T]:
    x: T

# TODO: no error
# TODO: revealed: C[int]
# error: [non-subscriptable]
reveal_type(C[int]())  # revealed: C
```

We can infer the type parameter from a type context:

```py
c: C[int] = C()
# TODO: revealed: C[int]
reveal_type(c)  # revealed: C
```

The typevars of a fully specialized generic class should no longer be visible:

```py
# TODO: revealed: int
reveal_type(c.x)  # revealed: T
```

If the type parameter is not specified explicitly, and there are no constraints that let us infer a
specific type, we infer the typevar's default type:

```py
class D[T = int]: ...

# TODO: revealed: D[int]
reveal_type(D())  # revealed: D
```

If a typevar does not provide a default, we use `Unknown`:

```py
# TODO: revealed: C[Unknown]
reveal_type(C())  # revealed: C
```

If the type of a constructor parameter is a class typevar, we can use that to infer the type
parameter:

```py
class E[T]:
    def __init__(self, x: T) -> None: ...

# TODO: revealed: E[int] or E[Literal[1]]
reveal_type(E(1))  # revealed: E
```

The types inferred from a type context and from a constructor parameter must be consistent with each
other:

```py
# TODO: error
wrong_innards: E[int] = E("five")
```

## Generic subclass

When a generic subclass fills its superclass's type parameter with one of its own, the actual types
propagate through:

```py
class Base[T]:
    x: T | None = None

# TODO: no error
# error: [non-subscriptable]
class Sub[U](Base[U]): ...

# TODO: no error
# TODO: revealed: int | None
# error: [non-subscriptable]
reveal_type(Base[int].x)  # revealed: T | None
# TODO: revealed: int | None
# error: [non-subscriptable]
reveal_type(Sub[int].x)  # revealed: T | None
```

## Cyclic class definition

A class can use itself as the type parameter of one of its superclasses. (This is also known as the
[curiously recurring template pattern][crtp] or [F-bounded quantification][f-bound].)

Here, `Sub` is not a generic class, since it fills its superclass's type parameter (with itself).

`stub.pyi`:

```pyi
class Base[T]: ...
# TODO: no error
# error: [non-subscriptable]
class Sub(Base[Sub]): ...

reveal_type(Sub)  # revealed: Literal[Sub]
```

A similar case can work in a non-stub file, if forward references are stringified:

`string_annotation.py`:

```py
class Base[T]: ...

# TODO: no error
# error: [non-subscriptable]
class Sub(Base["Sub"]): ...

reveal_type(Sub)  # revealed: Literal[Sub]
```

In a non-stub file, without stringified forward references, this raises a `NameError`:

`bare_annotation.py`:

```py
class Base[T]: ...

# TODO: the unresolved-reference error is correct, the non-subscriptable is not
# error: [non-subscriptable]
# error: [unresolved-reference]
class Sub(Base[Sub]): ...
```

## Another cyclic case

```pyi
# TODO no error (generics)
# error: [invalid-base]
class Derived[T](list[Derived[T]]): ...
```

[crtp]: https://en.wikipedia.org/wiki/Curiously_recurring_template_pattern
[f-bound]: https://en.wikipedia.org/wiki/Bounded_quantification#F-bounded_quantification
