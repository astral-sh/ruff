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

```py
class A:
    __slots__ = ("a", "b")

class B:
    __slots__ = ("c", "d")

class C(
    A,  # error: [incompatible-slots]
    B,  # error: [incompatible-slots]
): ...
```

## Same value

```py
class A:
    __slots__ = ("a", "b")

class B:
    __slots__ = ("a", "b")

class C(
    A,  # error: [incompatible-slots]
    B,  # error: [incompatible-slots]
): ...
```

## Strings

```py
class A:
    __slots__ = "abc"

class B:
    __slots__ = ("abc",)

class AB(
    A,  # error: [incompatible-slots]
    B,  # error: [incompatible-slots]
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
class E(
    B,  # error: [incompatible-slots]
    D,  # error: [incompatible-slots]
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

class C(
    A,  # error: [incompatible-slots]
    B,  # error: [incompatible-slots]
): ...
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

### Built-ins with implicit layouts

```py
# False negative: [incompatible-slots]
class A(int, str): ...
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
