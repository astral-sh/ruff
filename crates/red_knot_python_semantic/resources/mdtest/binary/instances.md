# Binary operations on instances

Binary operations in Python are implemented by means of magic double-underscore methods.

For references, see:

- <https://snarky.ca/unravelling-binary-arithmetic-operations-in-python/>
- <https://docs.python.org/3/reference/datamodel.html#emulating-numeric-types>

## Operations

We support inference for all Python's binary operators: `+`, `-`, `*`, `@`, `/`, `//`, `%`, `**`,
`<<`, `>>`, `&`, `^`, and `|`.

```py
class A:
    def __add__(self, other) -> "A":
        return self

    def __sub__(self, other) -> "A":
        return self

    def __mul__(self, other) -> "A":
        return self

    def __matmul__(self, other) -> "A":
        return self

    def __truediv__(self, other) -> "A":
        return self

    def __floordiv__(self, other) -> "A":
        return self

    def __mod__(self, other) -> "A":
        return self

    def __pow__(self, other) -> "A":
        return self

    def __lshift__(self, other) -> "A":
        return self

    def __rshift__(self, other) -> "A":
        return self

    def __and__(self, other) -> "A":
        return self

    def __xor__(self, other) -> "A":
        return self

    def __or__(self, other) -> "A":
        return self

class B: ...

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
    def __radd__(self, other) -> "A":
        return self

    def __rsub__(self, other) -> "A":
        return self

    def __rmul__(self, other) -> "A":
        return self

    def __rmatmul__(self, other) -> "A":
        return self

    def __rtruediv__(self, other) -> "A":
        return self

    def __rfloordiv__(self, other) -> "A":
        return self

    def __rmod__(self, other) -> "A":
        return self

    def __rpow__(self, other) -> "A":
        return self

    def __rlshift__(self, other) -> "A":
        return self

    def __rrshift__(self, other) -> "A":
        return self

    def __rand__(self, other) -> "A":
        return self

    def __rxor__(self, other) -> "A":
        return self

    def __ror__(self, other) -> "A":
        return self

class B: ...

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

## Returning a different type

The magic methods aren't required to return the type of `self`:

```py
class A:
    def __add__(self, other) -> int:
        return 1

    def __rsub__(self, other) -> int:
        return 1

class B: ...

reveal_type(A() + B())  # revealed: int
reveal_type(B() - A())  # revealed: int
```

## Non-reflected precedence in general

In general, if the left-hand side defines `__add__` and the right-hand side defines `__radd__` and
the right-hand side is not a subtype of the left-hand side, `lhs.__add__` will take precedence:

```py
class A:
    def __add__(self, other: "B") -> int:
        return 42

class B:
    def __radd__(self, other: "A") -> str:
        return "foo"

reveal_type(A() + B())  # revealed:  int

# Edge case: C is a subtype of C, *but* if the two sides are of *equal* types,
# the lhs *still* takes precedence
class C:
    def __add__(self, other: "C") -> int:
        return 42

    def __radd__(self, other: "C") -> str:
        return "foo"

reveal_type(C() + C())  # revealed: int
```

## Reflected precedence for subtypes (in some cases)

If the right-hand operand is a subtype of the left-hand operand and has a different implementation
of the reflected method, the reflected method on the right-hand operand takes precedence.

```py
class A:
    def __add__(self, other) -> str:
        return "foo"

    def __radd__(self, other) -> str:
        return "foo"

class MyString(str): ...

class B(A):
    def __radd__(self, other) -> MyString:
        return MyString()

reveal_type(A() + B())  # revealed: MyString

# N.B. Still a subtype of `A`, even though `A` does not appear directly in the class's `__bases__`
class C(B): ...

reveal_type(A() + C())  # revealed: MyString
```

## Reflected precedence 2

If the right-hand operand is a subtype of the left-hand operand, but does not override the reflected
method, the left-hand operand's non-reflected method still takes precedence:

```py
class A:
    def __add__(self, other) -> str:
        return "foo"

    def __radd__(self, other) -> int:
        return 42

class B(A): ...

reveal_type(A() + B())  # revealed: str
```

## Only reflected supported

For example, at runtime, `(1).__add__(1.2)` is `NotImplemented`, but `(1.2).__radd__(1) == 2.2`,
meaning that `1 + 1.2` succeeds at runtime (producing `2.2`). The runtime tries the second one only
if the first one returns `NotImplemented` to signal failure.

Typeshed and other stubs annotate dunder-method calls that would return `NotImplemented` as being
"illegal" calls. `int.__add__` is annotated as only "accepting" `int`s, even though it
strictly-speaking "accepts" any other object without raising an exception -- it will simply return
`NotImplemented`, allowing the runtime to try the `__radd__` method of the right-hand operand as
well.

```py
class A:
    def __sub__(self, other: "A") -> "A":
        return A()

class B:
    def __rsub__(self, other: A) -> "B":
        return B()

reveal_type(A() - B())  # revealed: B
```

## Callable instances as dunders

Believe it or not, this is supported at runtime:

```py
class A:
    def __call__(self, other) -> int:
        return 42

class B:
    __add__ = A()

reveal_type(B() + B())  # revealed: Unknown | int
```

Note that we union with `Unknown` here because `__add__` is not declared. We do infer just `int` if
the callable is declared:

```py
class B2:
    __add__: A = A()

reveal_type(B2() + B2())  # revealed: int
```

## Integration test: numbers from typeshed

We get less precise results from binary operations on float/complex literals due to the special case
for annotations of `float` or `complex`, which applies also to return annotations for typeshed
dunder methods. Perhaps we could have a special-case on the special-case, to exclude these typeshed
return annotations from the widening, and preserve a bit more precision here?

```py
reveal_type(3j + 3.14)  # revealed: int | float | complex
reveal_type(4.2 + 42)  # revealed: int | float
reveal_type(3j + 3)  # revealed: int | float | complex
reveal_type(3.14 + 3j)  # revealed: int | float | complex
reveal_type(42 + 4.2)  # revealed: int | float
reveal_type(3 + 3j)  # revealed: int | float | complex

def _(x: bool, y: int):
    reveal_type(x + y)  # revealed: int
    reveal_type(4.2 + x)  # revealed: int | float
    reveal_type(y + 4.12)  # revealed: int | float
```

## With literal types

When we have a literal type for one operand, we're able to fall back to the instance handling for
its instance super-type.

```py
class A:
    def __add__(self, other) -> "A":
        return self

    def __radd__(self, other) -> "A":
        return self

reveal_type(A() + 1)  # revealed: A
reveal_type(1 + A())  # revealed: A

reveal_type(A() + "foo")  # revealed: A
# TODO should be `A` since `str.__add__` doesn't support `A` instances
# TODO overloads
reveal_type("foo" + A())  # revealed: @Todo(return type of overloaded function)

reveal_type(A() + b"foo")  # revealed: A
# TODO should be `A` since `bytes.__add__` doesn't support `A` instances
reveal_type(b"foo" + A())  # revealed: bytes

reveal_type(A() + ())  # revealed: A
# TODO this should be `A`, since `tuple.__add__` doesn't support `A` instances
reveal_type(() + A())  # revealed: @Todo(return type of overloaded function)

literal_string_instance = "foo" * 1_000_000_000
# the test is not testing what it's meant to be testing if this isn't a `LiteralString`:
reveal_type(literal_string_instance)  # revealed: LiteralString

reveal_type(A() + literal_string_instance)  # revealed: A
# TODO should be `A` since `str.__add__` doesn't support `A` instances
# TODO overloads
reveal_type(literal_string_instance + A())  # revealed: @Todo(return type of overloaded function)
```

## Operations involving instances of classes inheriting from `Any`

`Any` and `Unknown` represent a set of possible runtime objects, wherein the bounds of the set are
unknown. Whether the left-hand operand's dunder or the right-hand operand's reflected dunder depends
on whether the right-hand operand is an instance of a class that is a subclass of the left-hand
operand's class and overrides the reflected dunder. In the following example, because of the
unknowable nature of `Any`/`Unknown`, we must consider both possibilities: `Any`/`Unknown` might
resolve to an unknown third class that inherits from `X` and overrides `__radd__`; but it also might
not. Thus, the correct answer here for the `reveal_type` is `int | Unknown`.

```py
from does_not_exist import Foo  # error: [unresolved-import]

reveal_type(Foo)  # revealed: Unknown

class X:
    def __add__(self, other: object) -> int:
        return 42

class Y(Foo): ...

# TODO: Should be `int | Unknown`; see above discussion.
reveal_type(X() + Y())  # revealed: int
```

## Operations involving types with invalid `__bool__` methods

<!-- snapshot-diagnostics -->

```py
class NotBoolable:
    __bool__: int = 3

a = NotBoolable()

# error: [unsupported-bool-conversion]
10 and a and True
```

## Operations on class objects

When operating on class objects, the corresponding dunder methods are looked up on the metaclass.

```py
from __future__ import annotations

class Meta(type):
    def __add__(self, other: Meta) -> int:
        return 1

    def __lt__(self, other: Meta) -> bool:
        return True

    def __getitem__(self, key: int) -> str:
        return "a"

class A(metaclass=Meta): ...
class B(metaclass=Meta): ...

reveal_type(A + B)  # revealed: int
# error: [unsupported-operator] "Operator `-` is unsupported between objects of type `Literal[A]` and `Literal[B]`"
reveal_type(A - B)  # revealed: Unknown

reveal_type(A < B)  # revealed: bool
reveal_type(A > B)  # revealed: bool

# error: [unsupported-operator] "Operator `<=` is not supported for types `Literal[A]` and `Literal[B]`"
reveal_type(A <= B)  # revealed: Unknown

reveal_type(A[0])  # revealed: str
```

## Unsupported

### Dunder as instance attribute

The magic method must exist on the class, not just on the instance:

```py
def add_impl(self, other) -> int:
    return 1

class A:
    def __init__(self):
        self.__add__ = add_impl

# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `A` and `A`"
# revealed: Unknown
reveal_type(A() + A())
```

### Missing dunder

```py
class A: ...

# error: [unsupported-operator]
# revealed: Unknown
reveal_type(A() + A())
```

### Wrong position

A left-hand dunder method doesn't apply for the right-hand operand, or vice versa:

```py
class A:
    def __add__(self, other) -> int:
        return 1

class B:
    def __radd__(self, other) -> int:
        return 1

class C: ...

# error: [unsupported-operator]
# revealed: Unknown
reveal_type(C() + A())

# error: [unsupported-operator]
# revealed: Unknown
reveal_type(B() + C())
```

### Reflected dunder is not tried between two objects of the same type

For the specific case where the left-hand operand is the exact same type as the right-hand operand,
the reflected dunder of the right-hand operand is not tried; the runtime short-circuits after trying
the unreflected dunder of the left-hand operand. For context, see
[this mailing list discussion](https://mail.python.org/archives/list/python-dev@python.org/thread/7NZUCODEAPQFMRFXYRMGJXDSIS3WJYIV/).

```py
class Foo:
    def __radd__(self, other: "Foo") -> "Foo":
        return self

# error: [unsupported-operator]
# revealed: Unknown
reveal_type(Foo() + Foo())
```

### Wrong type

TODO: check signature and error if `other` is the wrong type
