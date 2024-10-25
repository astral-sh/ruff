# Comparison: Rich Comparison

## Compare to same type

```py
class A:
    def __eq__(self, other: A) -> bool: ...
    def __ne__(self, other: A) -> bool: ...
    def __lt__(self, other: A) -> bool: ...
    def __le__(self, other: A) -> bool: ...
    def __gt__(self, other: A) -> bool: ...
    def __ge__(self, other: A) -> bool: ...


reveal_type(A() == A())  # revealed: bool
reveal_type(A() != A())  # revealed: bool
reveal_type(A() < A())  # revealed: bool
reveal_type(A() <= A())  # revealed: bool
reveal_type(A() > A())  # revealed: bool
reveal_type(A() >= A())  # revealed: bool
```

## Compare to other type

```py
class A:
    def __eq__(self, other: B) -> bool: ...
    def __ne__(self, other: B) -> bool: ...
    def __lt__(self, other: B) -> bool: ...
    def __le__(self, other: B) -> bool: ...
    def __gt__(self, other: B) -> bool: ...
    def __ge__(self, other: B) -> bool: ...


class B: ...


reveal_type(A() == B())  # revealed: bool
reveal_type(A() != B())  # revealed: bool
reveal_type(A() < B())  # revealed: bool
reveal_type(A() <= B())  # revealed: bool
reveal_type(A() > B())  # revealed: bool
reveal_type(A() >= B())  # revealed: bool
```

## Reflected Comparisons

```py
class A:
    def __eq__(self, other: B) -> A: ...
    def __ne__(self, other: B) -> A: ...
    def __lt__(self, other: B) -> A: ...
    def __le__(self, other: B) -> A: ...
    def __gt__(self, other: B) -> A: ...
    def __ge__(self, other: B) -> A: ...


class B:
    # To override builtins.object.__eq__ and builtins.object.__ne__
    def __eq__(self, other: str) -> B: ...
    def __ne__(self, other: str) -> B: ...


# TODO: should be `A`, need to check arg type and fall back to `rhs.__eq__` and `rhs.__ne__`
reveal_type(B() == A())  # revealed: B
reveal_type(B() != A())  # revealed: B

reveal_type(B() < A())  # revealed: A
reveal_type(B() <= A())  # revealed: A
reveal_type(B() > A())  # revealed: A
reveal_type(B() >= A())  # revealed: A


class C:
    def __gt__(self, other: C) -> C: ...
    def __ge__(self, other: C) -> C: ...


reveal_type(C() < C())  # revealed: C
reveal_type(C() <= C())  # revealed: C
```

## Reflected Comparisons with Subclasses

```py
class A:
    def __eq__(self, other: A) -> A: ...
    def __ne__(self, other: A) -> A: ...
    def __lt__(self, other: A) -> A: ...
    def __le__(self, other: A) -> A: ...
    def __gt__(self, other: A) -> A: ...
    def __ge__(self, other: A) -> A: ...


class B(A):
    def __eq__(self, other: A) -> B: ...
    def __ne__(self, other: A) -> B: ...
    def __lt__(self, other: A) -> B: ...
    def __le__(self, other: A) -> B: ...
    def __gt__(self, other: A) -> B: ...
    def __ge__(self, other: A) -> B: ...


reveal_type(A() == B())  # revealed: B
reveal_type(A() != B())  # revealed: B
reveal_type(A() < B())  # revealed: B
reveal_type(A() <= B())  # revealed: B
reveal_type(A() > B())  # revealed: B
reveal_type(A() >= B())  # revealed: B
```

## Reflected Comparisons with Subclass But Falls Back to LHS

```py
class A:
    def __lt__(self, other: A) -> A: ...
    def __gt__(self, other: A) -> A: ...


class B(A):
    def __lt__(self, other: int) -> B: ...
    def __gt__(self, other: int) -> B: ...


# TODO: should be `A`, need to check argument type and fall back to LHS method
reveal_type(A() < B())  # revealed: B
reveal_type(A() > B())  # revealed: B
```

## Operations involving instances of classes inheriting from `Any`

`Any` and `Unknown` represent a set of possible runtime objects, wherein the
bounds of the set are unknown. Whether the left-hand operand's dunder or the
right-hand operand's reflected dunder depends on whether the right-hand operand
is an instance of a class that is a subclass of the left-hand operand's class
and overrides the reflected dunder. In the following example, because of the
unknowable nature of `Any`/`Unknown`, we must consider both possibilities:
`Any`/`Unknown` might resolve to an unknown third class that inherits from `X`
and overrides `__gt__`; but it also might not. Thus, the correct answer here
for the `reveal_type` is `int | Unknown`.

```py
from does_not_exist import Foo  # error: [unresolved-import]

reveal_type(Foo)  # revealed: Unknown


class X:
    def __lt__(self, other: object) -> int:
        return 42


class Y(Foo): ...


# TODO: Should be `int | Unknown`; see above discussion.
reveal_type(X() < Y())  # revealed: int
```

## Object Comparisons with Typeshed

```py
class A: ...


reveal_type(A() == object())  # revealed: bool
reveal_type(A() != object())  # revealed: bool
reveal_type(object() == A())  # revealed: bool
reveal_type(object() != A())  # revealed: bool

# error: "Operator `<` is not supported for types `A` and `object`"
# revealed: Unknown
reveal_type(A() < object())
```

## Equality and Inequality Fallback

```py
class A:
    def __eq__(self, other: int) -> A: ...
    def __ne__(self, other: int) -> A: ...


# TODO: it should be `bool`, need to check arg type and fall back to `is` and `is not`
reveal_type(A() == A())  # revealed: A
reveal_type(A() != A())  # revealed: A
```

## Numbers Comparison with typeshed

```py
reveal_type(1 == 1.0)  # revealed: bool
reveal_type(1 != 1.0)  # revealed: bool
reveal_type(1 < 1.0)  # revealed: bool
reveal_type(1 <= 1.0)  # revealed: bool
reveal_type(1 > 1.0)  # revealed: bool
reveal_type(1 >= 1.0)  # revealed: bool


reveal_type(1 == 2j)  # revealed: bool
reveal_type(1 != 2j)  # revealed: bool

# TODO: should be Unknown, need to check arg type and should be failed
reveal_type(1 < 2j)  # revealed: bool
reveal_type(1 <= 2j)  # revealed: bool
reveal_type(1 > 2j)  # revealed: bool
reveal_type(1 >= 2j)  # revealed: bool


def bool_instance() -> bool: ...
def int_instance() -> int: ...


x = bool_instance()
y = int_instance()

reveal_type(x < y)  # revealed: bool
reveal_type(y < x)  # revealed: bool
reveal_type(4.2 < x)  # revealed: bool
reveal_type(x < 4.2)  # revealed: bool
```
