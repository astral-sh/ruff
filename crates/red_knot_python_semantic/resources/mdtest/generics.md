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

# TODO reveal int, do not leak the typevar
reveal_type(box.data)  # revealed: T

reveal_type(MyBox.box_model_number)  # revealed: Unknown | Literal[695]
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
# The @Todo(…) is misleading here. We currently treat `MyBox[T]` as a dynamic base class because we
# don't understand generics and therefore infer `Unknown` for the `MyBox[T]` base of `MySecureBox[T]`.
reveal_type(secure_box.data)  # revealed: @Todo(instance attribute on class with dynamic base)
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

A PEP695 type variable defines a value of type `typing.TypeVar`.

```py
def f[T]():
    reveal_type(T)  # revealed: T
    reveal_type(T.__name__)  # revealed: Literal["T"]
```

## Minimum two constraints

A typevar with less than two constraints emits a diagnostic:

```py
# error: [invalid-type-variable-constraints] "TypeVar must have at least two constrained types"
def f[T: (int,)]():
    pass
```
