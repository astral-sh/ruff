# `no-method-argument` (`PLE0211`)

```toml
lint.preview = true
lint.select = ["PLE0211"]
```

## Instance methods

A bound instance method receives the instance as an implicit positional argument.

```py
class Example:
    def method():  # snapshot: no-method-argument
        return 1

    async def async_method():  # error: [no-method-argument]
        return 1
```

```snapshot
error[PLE0211]: Method `method` has no parameter for its implicit argument
 --> src/mdtest_snippet.py:2:9
  |
2 |     def method():  # snapshot: no-method-argument
  |         ^^^^^^
```

## Functions and methods that accept a receiver

Functions outside a class do not receive an implicit argument.
A positional parameter or `*args` can receive the implicit argument of a method.

```py
def function():  # no diagnostic
    return 1

class Example:
    def method(self):  # no diagnostic
        def inner():  # no diagnostic
            return 1
        return inner()

    def positional_only(self, /):  # no diagnostic
        return 1

    def variadic(*args):  # no diagnostic
        return 1
```

## Keyword parameters

Methods with any parameters are ignored, including keyword-only parameters and
`**kwargs`, even though they cannot receive a positional receiver.

```py
class Example:
    def keyword_only(*, self):  # no diagnostic
        return 1

    def kwargs_only(**kwargs):  # no diagnostic
        return 1
```

## Decorated methods

Decorators can change how a method is called, so methods with any decorators are ignored.
This includes identity decorators that leave the function unchanged.

```py
def decorate(function):
    return staticmethod(function)

def identity(function):
    return function

class Example:
    @staticmethod
    def static_method():  # no diagnostic
        return 1

    @classmethod
    def class_method():  # no diagnostic
        return 1

    @decorate
    def decorated():  # no diagnostic
        return 1

    @identity
    def identity_decorated():  # no diagnostic
        return 1
```

## Special methods

`__new__` receives the class explicitly from Python's constructor machinery.
`__init_subclass__` and `__class_getitem__` are implicitly class methods.

```py
class Example:
    def __new__():  # error: [no-method-argument]
        return object.__new__(Example)

    def __init__():  # error: [no-method-argument]
        pass

    def __init_subclass__():  # error: [no-method-argument]
        pass

    def __class_getitem__():  # error: [no-method-argument]
        return 1

class Meta(type):
    def method():  # error: [no-method-argument]
        return 1
```

## Intentional class-only calls

A function accessed directly on the class can be called without an instance.
The rule still flags its definition because calling it through an instance fails.
An explicit `staticmethod` or a suppression can express that this is intentional.

```py
class Example:
    def method():  # error: [no-method-argument]
        return 1

    def intentional():  # noqa: PLE0211
        return 1

Example.method()
Example.intentional()
```
