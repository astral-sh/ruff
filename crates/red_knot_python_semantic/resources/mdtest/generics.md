# PEP 695 Generics

## Class Declarations

Basic PEP 695 generics

```py
class MyBox[T]:
    data: T
    box_model_number = 695

    def __init__(self, data: T):
        self.data = data

box: MyBox[int] = MyBox(5)

# TODO should emit a diagnostic here (str is not assignable to int)
wrong_innards: MyBox[int] = MyBox("five")

# TODO reveal int
reveal_type(box.data)  # revealed: @Todo

reveal_type(MyBox.box_model_number)  # revealed: Literal[695]
```

## Subclassing

```py
class MyBox[T]:
    data: T

    def __init__(self, data: T):
        self.data = data

# TODO not error on the subscripting
# error: [non-subscriptable]
class MySecureBox[T](MyBox[T]): ...

secure_box: MySecureBox[int] = MySecureBox(5)
reveal_type(secure_box)  # revealed: MySecureBox
# TODO reveal int
reveal_type(secure_box.data)  # revealed: @Todo
```

## Cyclical class definition

In type stubs, classes can reference themselves in their base class definitions. For example, in
`typeshed`, we have `class str(Sequence[str]): ...`.

This should hold true even with generics at play.

```py path=a.pyi
class Seq[T]: ...

# TODO not error on the subscripting
class S[T](Seq[S]): ...  # error: [non-subscriptable]

reveal_type(S)  # revealed: Literal[S]
```

## Type params

A PEP695 type variable defines a value of type `typing.TypeVar` with attributes `__name__`,
`__bounds__`, `__constraints__`, and `__default__` (the latter three all lazily evaluated):

```py
def f[T, U: A, V: (A, B), W = A, X: A = A1]():
    reveal_type(T)  # revealed: TypeVar
    reveal_type(T.__name__)  # revealed: Literal["T"]
    reveal_type(T.__bound__)  # revealed: None
    reveal_type(T.__constraints__)  # revealed: tuple[()]
    reveal_type(T.__default__)  # revealed: NoDefault

    reveal_type(U)  # revealed: TypeVar
    reveal_type(U.__name__)  # revealed: Literal["U"]
    reveal_type(U.__bound__)  # revealed: type[A]
    reveal_type(U.__constraints__)  # revealed: tuple[()]
    reveal_type(U.__default__)  # revealed: NoDefault

    reveal_type(V)  # revealed: TypeVar
    reveal_type(V.__name__)  # revealed: Literal["V"]
    reveal_type(V.__bound__)  # revealed: None
    reveal_type(V.__constraints__)  # revealed: tuple[type[A], type[B]]
    reveal_type(V.__default__)  # revealed: NoDefault

    reveal_type(W)  # revealed: TypeVar
    reveal_type(W.__name__)  # revealed: Literal["W"]
    reveal_type(W.__bound__)  # revealed: None
    reveal_type(W.__constraints__)  # revealed: tuple[()]
    reveal_type(W.__default__)  # revealed: type[A]

    reveal_type(X)  # revealed: TypeVar
    reveal_type(X.__name__)  # revealed: Literal["X"]
    reveal_type(X.__bound__)  # revealed: type[A]
    reveal_type(X.__constraints__)  # revealed: tuple[()]
    reveal_type(X.__default__)  # revealed: type[A1]

class A: ...
class B: ...
class A1(A): ...
```

## Minimum two constraints

A typevar with less than two constraints emits a diagnostic and is treated as unconstrained:

```py
# error: [invalid-typevar-constraints] "TypeVar must have at least two constrained types"
def f[T: (int,)]():
    reveal_type(T.__constraints__)  # revealed: tuple[()]
```
