# `__slots__`

## Not specified and empty

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

## Incompatible tuples

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

## Same value

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

## Strings

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

## Invalid

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

## Inheritance

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

## Single solid base

```py
class A:
    __slots__ = ("a", "b")

class B(A): ...
class C(A): ...
class D(B, A): ...  # fine
class E(B, C, A): ...  # fine
```

## Post-hoc modifications

```py
class A:
    __slots__ = ()
    __slots__ += ("a", "b")

reveal_type(A.__slots__)  # revealed: tuple[Literal["a"], Literal["b"]]

class B:
    __slots__ = ("c", "d")

class C(  # error: [instance-layout-conflict]
    A,
    B,
): ...
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

Certain classes implemented in C extensions are also considered "solid bases" in the same way as
classes that define non-empty `__slots__`. There is no generalized way for ty to detect if a class
is a "solid base", but ty special-cases certain builtin classes:

```py
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

# fmt: on
```

We avoid emitting an `instance-layout-conflict` diagnostic for this class definition, because
`range` is `@final`, so we'll complain about the `class` statement anyway:

```py
class Foo(range, str): ...  # error: [subclass-of-final-class]
```

## Multiple solid bases where one is a subclass of the other

A class is permitted to multiple-inherit from multiple solid bases if one is a subclass of the
other:

```py
class A:
    __slots__ = ("a",)

class B(A):
    __slots__ = ("b",)

class C(B, A): ...  # fine
```

## False negatives

### Possibly unbound

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

### Bound but with different types

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

### Non-tuples

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
