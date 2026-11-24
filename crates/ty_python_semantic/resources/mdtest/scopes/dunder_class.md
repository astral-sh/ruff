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

## Lambda scopes

Lambdas defined directly in a class body also capture the cell. Lambda parameters continue to take
precedence over it.

```py
class C:
    lambda_method = lambda: reveal_type(__class__)  # revealed: <class 'C'>
    shadowed = lambda __class__: reveal_type(__class__)  # revealed: Unknown
```

## Generator expression scopes

The body of a generator expression defined directly in a class captures the cell because it is
evaluated lazily. The first iterable is evaluated eagerly in the class body, where the cell is not
yet available. Eager comprehension bodies likewise cannot access the cell.

```py
class C:
    values = (
        reveal_type(__class__)  # revealed: <class 'C'>
        for _ in range(1)
    )

    first_iterable = (
        value
        for value in (
            __class__,  # error: [unresolved-reference]
        )
    )

    eager_comprehension = [
        __class__  # error: [unresolved-reference]
        for _ in range(1)
    ]
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

## Known limitations

The implicit cell is currently only modeled in direct method bodies and lazy scopes defined directly
in a class body. The following valid uses are left unresolved until the cell can be represented at
the correct lexical scope boundary.

### Nested function scopes

```py
class C:
    def method(self) -> None:
        def nested() -> None:
            # TODO: This should reveal `<class 'C'>` without an error.
            # error: [unresolved-reference]
            # revealed: Unknown
            reveal_type(__class__)
```

### Type alias annotation scopes

```toml
[environment]
python-version = "3.12"
```

```py
class C:
    # TODO: This should resolve to `C` without an error.
    type Alias = __class__  # error: [unresolved-reference]

    # TODO: This should resolve to `C` without an error.
    type GenericAlias[T] = __class__  # error: [unresolved-reference]
```

### Generic method bounds

```toml
[environment]
python-version = "3.12"
```

```py
class C:
    # TODO: The bound should resolve to `C` without an error.
    def method[T: __class__](self) -> None: ...  # error: [unresolved-reference]
```

### Deferred method annotations

Python 3.14 defers annotation evaluation, so ordinary method annotations can access the cell.

```toml
[environment]
python-version = "3.14"
```

```py
class C:
    def method(
        self,
        # TODO: This should resolve to `C` without an error.
        value: __class__,  # error: [unresolved-reference]
        # TODO: This should resolve to `C` without an error.
    ) -> __class__:  # error: [unresolved-reference]
        raise NotImplementedError
```
