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

### Post-hoc modifications

```py
class A:
    __slots__ = ()
    __slots__ += ("a", "b")

reveal_type(A.__slots__)  # revealed: @Todo(return type)

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
