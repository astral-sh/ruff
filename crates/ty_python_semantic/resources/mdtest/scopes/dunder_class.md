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

    lambda_method = lambda self: reveal_type(__class__)  # revealed: <class 'C'>
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

## Nested scopes

The cell is captured by scopes nested inside a method. A method defined in a nested class receives a
new cell for that class.

```py
class Outer:
    def method(self) -> None:
        def nested() -> None:
            reveal_type(__class__)  # revealed: <class 'Outer'>

        class Inner:
            reveal_type(__class__)  # revealed: <class 'Outer'>

            def method(self) -> None:
                reveal_type(__class__)  # revealed: <class 'Inner'>
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

## Lexical precedence

The cell is located between the method and class scopes. It takes precedence over bindings outside
the class, but an intervening `global` declaration redirects lookups to the module scope.

```py
def outer() -> None:
    __class__ = int

    class C:
        def method(self) -> None:
            reveal_type(__class__)  # revealed: <class 'C'>

__class__ = int

class D:
    def method(self) -> None:
        global __class__

        def nested() -> None:
            reveal_type(__class__)  # revealed: <class 'int'>
```

## Type alias annotation scopes

PEP 695 type aliases defined in class bodies can access the class namespace. When no class-local
binding named `__class__` exists, the name refers to the containing class.

```toml
[environment]
python-version = "3.12"
```

```py
class Direct:
    type Alias = __class__

def direct(value: Direct.Alias) -> None:
    reveal_type(value)  # revealed: Direct

class Generic:
    type Alias[T] = __class__

def generic(value: Generic.Alias[int]) -> None:
    reveal_type(value)  # revealed: Generic

class Shadowed:
    __class__ = int
    type Alias = __class__

def shadowed(value: Shadowed.Alias) -> None:
    reveal_type(value)  # revealed: int

class Outer:
    def method(self) -> None:
        class Inner:
            type Alias = __class__

        def nested(value: Inner.Alias) -> None:
            reveal_type(value)  # revealed: Inner
```

## Generic method bounds

The type-parameter scope of a generic method can also access the cell.

```toml
[environment]
python-version = "3.12"
```

```py
class Generic:
    def method[T: __class__](self) -> None: ...
```

## Eager generic method annotations

On Python 3.12 and 3.13, ordinary method annotations are evaluated before the cell is initialized.

```toml
[environment]
python-version = "3.13"
```

```py
class C:
    def method[T](
        self,
        value: __class__,  # error: [unresolved-reference]
    ) -> None: ...
```

## Deferred generic method annotations

Python 3.14 defers annotation evaluation, making the cell available to ordinary annotations as well
as type-parameter bounds.

```toml
[environment]
python-version = "3.14"
```

```py
class C:
    def method[T](self, value: __class__) -> __class__:
        raise NotImplementedError
```
