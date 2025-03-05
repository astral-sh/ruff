# Constructor

## No init or new

Here we statically infer that there is no custom `__init__` or `__new__` method defined on the
class, hence the call to the constructor is only valid with no arguments.

```py
class Foo: ...

reveal_type(Foo())  # revealed: Foo

# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
Foo(1)
```

## `__init__` present on the class itself

If the class has an `__init__` method, we can infer the signature of the constructor from it.

```py
class Foo:
    def __init__(self, x: int): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
Foo()
# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 2, got 3"
Foo(1, 2)
```

## `__init__` present on a superclass

If the `__init__` method is defined on a superclass, we can still infer the signature of the
constructor from it.

```py
class Base:
    def __init__(self, x: int): ...

class Foo(Base): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
Foo()
# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 2, got 3"
Foo(1, 2)
```
