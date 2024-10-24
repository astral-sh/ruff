# PEP 695 Generics

## Class Declarations

Basic PEP 695 generics

```py
class MyBox[T]:
    data: T
    box_model_number = 695

    def __init__(self, data: T):
        self.data = data

# TODO not error (should be subscriptable)
box: MyBox[int] = MyBox(5)  # error: [non-subscriptable]
# TODO error differently (str and int don't unify)
wrong_innards: MyBox[int] = MyBox("five")  # error: [non-subscriptable]
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
class MySecureBox[T](MyBox[T]): ...  # error: [non-subscriptable]

secure_box: MySecureBox[int] = MySecureBox(5)
reveal_type(secure_box)  # revealed: MySecureBox
# TODO reveal int
reveal_type(secure_box.data)  # revealed: @Todo
```

## Cyclical class definition

In type stubs, classes can reference themselves in their base class definitions. For example, in `typeshed`, we have `class str(Sequence[str]): ...`.

This should hold true even with generics at play.

```py path=a.pyi
class Seq[T]: ...

# TODO not error on the subscripting
class S[T](Seq[S]): ...  # error: [non-subscriptable]

reveal_type(S)  # revealed: Literal[S]
```
