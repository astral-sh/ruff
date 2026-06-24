# The `__class__` closure cell

Python implicitly creates a closure cell named `__class__` for methods defined in a class body. The
cell is available in instance methods, static methods, and class methods.

## Method scopes

```py
class C:
    def method(self) -> None:
        reveal_type(__class__)  # revealed: <class 'C'>

    @staticmethod
    def static_method() -> None:
        reveal_type(__class__)  # revealed: <class 'C'>

    @classmethod
    def class_method(cls) -> None:
        reveal_type(__class__)  # revealed: <class 'C'>
```

## Class bodies and method defaults

The cell is not available directly in the class body or while evaluating a method's default
arguments.

```py
class C:
    __class__  # error: [unresolved-reference]

    def method(
        self,
        value=__class__,  # error: [unresolved-reference]
    ) -> None: ...
```

## Shadowing

The implicit cell takes precedence over a global with the same name. Local bindings and explicit
`global` declarations continue to take precedence over the cell.

```py
__class__ = int

class D:
    def implicit(self) -> None:
        reveal_type(__class__)  # revealed: <class 'D'>

    def local(self) -> None:
        __class__ = str
        reveal_type(__class__)  # revealed: <class 'str'>

    def explicit_global(self) -> None:
        global __class__
        reveal_type(__class__)  # revealed: <class 'int'>
```
