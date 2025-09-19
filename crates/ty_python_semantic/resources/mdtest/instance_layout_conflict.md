# Tests for ty's `instance-layout-conflict` error code

## `__slots__`: not specified or empty

```py
class A: ...

class B:
    __slots__ = ()

class C:
    __slots__ = ("lorem", "ipsum")

class AB(A, B): ...  # fine
class AC(A, C): ...  # fine
class BC(B, C): ...  # fine
class ABC(A, B, C): ...  # fine
```

## `__slots__`: incompatible tuples

<!-- snapshot-diagnostics -->

```py
class A:
    __slots__ = ("a", "b")

class B:
    __slots__ = ("c", "d")

class C(  # error: [instance-layout-conflict]
    A,
    B,
): ...
```

## `__slots__` are the same value

```py
class A:
    __slots__ = ("a", "b")

class B:
    __slots__ = ("a", "b")

class C(  # error: [instance-layout-conflict]
    A,
    B,
): ...
```

## `__slots__` is a string

```py
class A:
    __slots__ = "abc"

class B:
    __slots__ = ("abc",)

class AB(  # error: [instance-layout-conflict]
    A,
    B,
): ...
```

## Synthesized `__slots__` from dataclasses

```py
from dataclasses import dataclass

@dataclass(slots=True)
class F: ...

@dataclass(slots=True)
class G: ...

class H(F, G): ...  # fine because both classes have empty `__slots__`

@dataclass(slots=True)
class I:
    x: int

@dataclass(slots=True)
class J:
    y: int

class K(I, J): ...  # error: [instance-layout-conflict]
```

## Invalid `__slots__` definitions

TODO: Emit diagnostics

```py
class NonString1:
    __slots__ = 42

class NonString2:
    __slots__ = b"ar"

class NonIdentifier1:
    __slots__ = "42"

class NonIdentifier2:
    __slots__ = ("lorem", "42")

class NonIdentifier3:
    __slots__ = (e for e in ("lorem", "42"))
```

## Inherited `__slots__`

```py
class A:
    __slots__ = ("a", "b")

class B(A): ...

class C:
    __slots__ = ("c", "d")

class D(C): ...
class E(  # error: [instance-layout-conflict]
    B,
    D,
): ...
```

## A single "disjoint base"

```py
class A:
    __slots__ = ("a", "b")

class B(A): ...
class C(A): ...
class D(B, A): ...  # fine
class E(B, C, A): ...  # fine
```

## Post-hoc modifications to `__slots__`

```py
class A:
    __slots__ = ()
    __slots__ += ("a", "b")

reveal_type(A.__slots__)  # revealed: tuple[Literal["a", "b"], ...]

class B:
    __slots__ = ("c", "d")

# TODO: ideally this would trigger `[instance-layout-conflict]`
# (but it's also not high-priority)
class C(A, B): ...
```

## Explicitly annotated `__slots__`

We do not emit false positives on classes with empty `__slots__` definitions, even if the
`__slots__` definitions are annotated with variadic tuples:

```py
class Foo:
    __slots__: tuple[str, ...] = ()

class Bar:
    __slots__: tuple[str, ...] = ()

class Baz(Foo, Bar): ...  # fine
```

## Built-ins with implicit layouts

<!-- snapshot-diagnostics -->

Certain classes implemented in C extensions also have an extended instance memory layout, in the
same way as classes that define non-empty `__slots__`. CPython internally calls all such classes
with a unique instance memory layout "solid bases", but [PEP 800](https://peps.python.org/pep-0800/)
calls these classes "disjoint bases", and this is the term we generally use. The `@disjoint_base`
decorator introduced by this PEP provides a generalised way for type checkers to identify such
classes.

```py
from typing_extensions import disjoint_base

# fmt: off

class A(  # error: [instance-layout-conflict]
    int,
    str
): ...

class B:
    __slots__ = ("b",)

class C(  # error: [instance-layout-conflict]
    int,
    B,
): ...
class D(int): ...

class E(  # error: [instance-layout-conflict]
    D,
    str
): ...

class F(int, str, bytes, bytearray): ...  # error: [instance-layout-conflict]

@disjoint_base
class G: ...

@disjoint_base
class H: ...

class I(  # error: [instance-layout-conflict]
    G,
    H
): ...

# fmt: on
```

We avoid emitting an `instance-layout-conflict` diagnostic for this class definition, because
`range` is `@final`, so we'll complain about the `class` statement anyway:

```py
class Foo(range, str): ...  # error: [subclass-of-final-class]
```

## Multiple "disjoint bases" where one is a subclass of the other

A class is permitted to multiple-inherit from multiple disjoint bases if one is a subclass of the
other:

```py
class A:
    __slots__ = ("a",)

class B(A):
    __slots__ = ("b",)

class C(B, A): ...  # fine
```

The same principle, but a more complex example:

```py
class AA:
    __slots__ = ("a",)

class BB(AA):
    __slots__ = ("b",)

class CC(BB): ...
class DD(AA): ...
class FF(CC, DD): ...  # fine
```

## False negatives

### Possibly unbound `__slots__`

```py
def _(flag: bool):
    class A:
        if flag:
            __slots__ = ("a", "b")

    class B:
        __slots__ = ("c", "d")

    # Might or might not be fine at runtime
    class C(A, B): ...
```

### Bound `__slots__` but with different types

```py
def _(flag: bool):
    class A:
        if flag:
            __slots__ = ("a", "b")
        else:
            __slots__ = ()

    class B:
        __slots__ = ("c", "d")

    # Might or might not be fine at runtime
    class C(A, B): ...
```

### Non-tuple `__slots__` definitions

```py
class A:
    __slots__ = ["a", "b"]  # This is treated as "dynamic"

class B:
    __slots__ = ("c", "d")

# False negative: [incompatible-slots]
class C(A, B): ...
```

### Diagnostic if `__slots__` is externally modified

We special-case type inference for `__slots__` and return the pure inferred type, even if the symbol
is not declared — a case in which we union with `Unknown` for other public symbols. The reason for
this is that `__slots__` has a special handling in the runtime. Modifying it externally is actually
allowed, but those changes do not take effect. If you have a class `C` with `__slots__ = ("foo",)`
and externally set `C.__slots__ = ("bar",)`, you still can't access `C.bar`. And you can still
access `C.foo`. We therefore issue a diagnostic for such assignments:

```py
class A:
    __slots__ = ("a",)

    # Modifying `__slots__` from within the class body is fine:
    __slots__ = ("a", "b")

# No `Unknown` here:
reveal_type(A.__slots__)  # revealed: tuple[Literal["a"], Literal["b"]]

# But modifying it externally is not:

# error: [invalid-assignment]
A.__slots__ = ("a",)

# error: [invalid-assignment]
A.__slots__ = ("a", "b_new")

# error: [invalid-assignment]
A.__slots__ = ("a", "b", "c")
```
