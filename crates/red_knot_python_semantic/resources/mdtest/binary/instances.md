# Binary operations on instances

Binary operations in Python are implemented by means of magic double-underscore methods.

For references, see:

- <https://snarky.ca/unravelling-binary-arithmetic-operations-in-python/>
- <https://docs.python.org/3/reference/datamodel.html#emulating-numeric-types>

## Operations

We support inference for all Python's binary operators:
`+`, `-`, `*`, `@`, `/`, `//`, `%`, `**`, `<<`, `>>`, `&`, `^`, and `|`.

```py
class A:
    def __add__(self, other) -> A: return self
    def __sub__(self, other) -> A: return self
    def __mul__(self, other) -> A: return self
    def __matmul__(self, other) -> A: return self
    def __truediv__(self, other) -> A: return self
    def __floordiv__(self, other) -> A: return self
    def __mod__(self, other) -> A: return self
    def __pow__(self, other) -> A: return self
    def __lshift__(self, other) -> A: return self
    def __rshift__(self, other) -> A: return self
    def __and__(self, other) -> A: return self
    def __xor__(self, other) -> A: return self
    def __or__(self, other) -> A: return self

class B: pass

reveal_type(A() + B())  # revealed: A
reveal_type(A() - B())  # revealed: A
reveal_type(A() * B())  # revealed: A
reveal_type(A() @ B())  # revealed: A
reveal_type(A() / B())  # revealed: A
reveal_type(A() // B())  # revealed: A
reveal_type(A() % B())  # revealed: A
reveal_type(A() ** B())  # revealed: A
reveal_type(A() << B())  # revealed: A
reveal_type(A() >> B())  # revealed: A
reveal_type(A() & B())  # revealed: A
reveal_type(A() ^ B())  # revealed: A
reveal_type(A() | B())  # revealed: A
```

## Reflected

We also support inference for reflected operations:

```py
class A:
    def __radd__(self, other) -> A: return self
    def __rsub__(self, other) -> A: return self
    def __rmul__(self, other) -> A: return self
    def __rmatmul__(self, other) -> A: return self
    def __rtruediv__(self, other) -> A: return self
    def __rfloordiv__(self, other) -> A: return self
    def __rmod__(self, other) -> A: return self
    def __rpow__(self, other) -> A: return self
    def __rlshift__(self, other) -> A: return self
    def __rrshift__(self, other) -> A: return self
    def __rand__(self, other) -> A: return self
    def __rxor__(self, other) -> A: return self
    def __ror__(self, other) -> A: return self

class B: pass

reveal_type(B() + A())  # revealed: A
reveal_type(B() - A())  # revealed: A
reveal_type(B() * A())  # revealed: A
reveal_type(B() @ A())  # revealed: A
reveal_type(B() / A())  # revealed: A
reveal_type(B() // A())  # revealed: A
reveal_type(B() % A())  # revealed: A
reveal_type(B() ** A())  # revealed: A
reveal_type(B() << A())  # revealed: A
reveal_type(B() >> A())  # revealed: A
reveal_type(B() & A())  # revealed: A
reveal_type(B() ^ A())  # revealed: A
reveal_type(B() | A())  # revealed: A
```

## Return different type

The magic methods aren't required to return the type of `self`:

```py
class A:
    def __add__(self, other) -> int: return 1
    def __rsub__(self, other) -> int: return 1

class B:
    pass

reveal_type(A() + B())  # revealed: int
reveal_type(B() - A())  # revealed: int
```

## Non-reflected precedence in general

In general, if the left-hand side defines `__add__` and the right-hand side
defines `__radd__` and the right-hand side is not a subtype of the left-hand
side, `lhs.__add__` will take precedence:

```py
class A:
    def __add__(self, other: B) -> int: return 42

class B:
    def __radd__(self, other: A) -> str: return "foo"

# C is a subtype of C, but if the two sides are of equal types,
# the lhs *still* takes precedence
class C:
    def __add__(self, other: C) -> int: return 42
    def __radd__(self, other: C) -> str: return "foo"

reveal_type(A() + B())  # revealed:  int
reveal_type(C() + C())  # revealed: int
```

## Reflected precedence for subtypes (in some cases)

If the right-hand operand is a subtype of the left-hand operand and has a
different implementation of the reflected method, the reflected method on the
right-hand operand takes precedence.

```py
class A:
    def __add__(self, other) -> str: return "foo"
    def __radd__(self, other) -> str: return "foo"

class MyString(str): ...

class B(A):
    def __radd__(self, other) -> MyString: return MyString()

reveal_type(A() + B())  # revealed: MyString

# N.B. Still a subtype of `A`, even though `A` does not appear directly in the class's `__bases__`
class C(B): ...

# TODO: we currently only understand direct subclasses as subtypes of the superclass.
# We need to iterate through the full MRO rather than just the class's bases;
# if we do, we'll understand `C` as a subtype of `A`, and correctly understand this as being
# `MyString` rather than `str`
reveal_type(A() + C())  # revealed: str
```

## Reflected precedence 2

If the right-hand operand is a subtype of the left-hand operand, but does not
override the reflected method, the left-hand operand's non-reflected method
still takes precedence:

```py
class A:
    def __add__(self, other) -> str: return "foo"
    def __radd__(self, other) -> int: return 42

class B(A): pass

reveal_type(A() + B())  # revealed: str
```

## Only reflected supported

For example, at runtime, `(1).__add__(1.2)` is `NotImplemented`, but
`(1.2).__radd__(1) == 2.2`, meaning that `1 + 1.2` succeeds at runtime
(producing `2.2`).

```py
class A:
    def __sub__(self, other: A) -> A:
        return A()

class B:
    def __rsub__(self, other: A) -> B:
        return B()

A() - B()
```

## Callable instances as dunders

Believe it or not, this is supported at runtime:

```py
class A:
    def __call__(self, other) -> int:
        return 42

class B:
    __add__ = A()

reveal_type(B() + B())  # revealed: int
```

## Integration test: numbers from typeshed

```py
reveal_type(3j + 3.14)  # revealed: complex
reveal_type(4.2 + 42)  # revealed: float
reveal_type(3j + 3)  # revealed: complex

# TODO should be complex, need to check arg type and fall back
reveal_type(3.14 + 3j)  # revealed: float

# TODO should be float, need to check arg type and fall back
reveal_type(42 + 4.2)  # revealed: int

# TODO should be complex, need to check arg type and fall back
reveal_type(3 + 3j)  # revealed: int

def returns_int() -> int:
    return 42

def returns_bool() -> bool:
    return True

x = returns_bool()
y = returns_int()

reveal_type(x + y)  # revealed: int
reveal_type(4.2 + x)  # revealed: float

# TODO should be float, need to check arg type and fall back
reveal_type(y + 4.12)  # revealed: int
```

## With literal types

When we have a literal type for one operand, we're able to fall back to the
instance handling for its instance super-type.

```py
class A:
    def __add__(self, other) -> A: return self
    def __radd__(self, other) -> A: return self

reveal_type(A() + 1)  # revealed: A
reveal_type(1 + A())  # revealed: int
reveal_type(A() + "foo")  # revealed: A
# TODO should be A since `str.__add__` doesn't support A instances
reveal_type("foo" + A())  # revealed: @Todo
reveal_type(A() + b"foo")  # revealed: A
reveal_type(b"foo" + A())  # revealed: bytes
reveal_type(A() + ())  # revealed: A
# TODO this should be A, since tuple's `__add__` doesn't support A instances
reveal_type(() + A())  # revealed: @Todo
```

## Unsupported

### Dunder on instance

The magic method must be on the class, not just on the instance:

```py
def add_impl(self, other) -> int: return 1

class A:
    def __init__(self):
        self.__add__ = add_impl

# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `A` and `A`"
# revealed: Unknown
reveal_type(A() + A())
```

### Missing dunder

```py
class A: pass

# error: [unsupported-operator]
# revealed: Unknown
reveal_type(A() + A())
```

### Wrong position

A left-hand dunder method doesn't apply for the right-hand operand, or vice versa:

```py
class A:
    def __add__(self, other) -> int: ...

class B:
    def __radd__(self, other) -> int: ...

class C: pass

# error: [unsupported-operator]
# revealed: Unknown
reveal_type(C() + A())

# error: [unsupported-operator]
# revealed: Unknown
reveal_type(B() + C())
```

### Wrong type

TODO: check signature and error if `other` is the wrong type
