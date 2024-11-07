# PEP 695 Generics

## Class Declarations

Basic PEP 695 generics

```py
class MyBox[T]:
    # TODO: `T` is defined here
    # error: [unresolved-reference] "Name `T` used when not defined"
    data: T
    box_model_number = 695

    # TODO: `T` is defined here
    # error: [unresolved-reference] "Name `T` used when not defined"
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
    # TODO: `T` is defined here
    # error: [unresolved-reference] "Name `T` used when not defined"
    data: T

    # TODO: `T` is defined here
    # error: [unresolved-reference] "Name `T` used when not defined"
    def __init__(self, data: T):
        self.data = data

# TODO not error on the subscripting or the use of type param
# error: [unresolved-reference] "Name `T` used when not defined"
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
