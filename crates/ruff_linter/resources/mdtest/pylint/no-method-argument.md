# `no-method-argument` (`PLE0211`)

```toml
lint.preview = true
lint.select = ["PLE0211"]
```

## Instance and class methods

A bound method receives an instance or class as an implicit positional argument.

```py
class Example:
    def method():  # snapshot: no-method-argument
        return 1

    @classmethod
    def class_method():  # error: [no-method-argument]
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

Static methods and functions outside a class do not receive an implicit argument.
A positional parameter or `*args` can receive the implicit argument of a method.

```py
def function():  # no diagnostic
    return 1

class Example:
    @staticmethod
    def static_method():  # no diagnostic
        return 1

    def method(self):  # no diagnostic
        def inner():  # no diagnostic
            return 1
        return inner()

    def positional_only(self, /):  # no diagnostic
        return 1

    def variadic(*args):  # no diagnostic
        return 1

    @classmethod
    def class_method(cls):  # no diagnostic
        return 1

    @classmethod
    def variadic_class_method(*args):  # no diagnostic
        return 1
```

## Keyword-only parameters

Keyword-only parameters cannot receive a positional receiver. For compatibility
with Pylint's E0211, `**kwargs` is exempt even though it cannot receive one either.

```py
class Example:
    def keyword_only(*, self):  # error: [no-method-argument]
        return 1

    @classmethod
    def keyword_only_class_method(*, cls):  # error: [no-method-argument]
        return 1

    def kwargs_only(**kwargs):  # no diagnostic
        return 1

    def variadic(*args, keyword=None):  # no diagnostic
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

class ExplicitStaticNew:
    @staticmethod
    def __new__():  # error: [no-method-argument]
        return object.__new__(ExplicitStaticNew)

class ExplicitStaticInitSubclass:
    @staticmethod
    def __init_subclass__():  # no diagnostic
        pass
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

## Naming rules are independent

```toml
lint.select = ["N804", "N805"]
```

Selecting naming rules alone does not enable the missing-argument check.

```py
class Example:
    def method():  # no diagnostic
        return 1

    @classmethod
    def class_method():  # no diagnostic
        return 1

    def wrong_name(this):  # error: [invalid-first-argument-name-for-method]
        return 1

    @classmethod
    def wrong_class_name(self):  # error: [invalid-first-argument-name-for-class-method]
        return 1
```

## Custom method decorators

```toml
lint.preview = true
lint.select = ["PLE0211"]
lint.pep8-naming.staticmethod-decorators = ["example.static"]
lint.pep8-naming.classmethod-decorators = ["example.class_method"]
```

```py
from builtins import staticmethod as static
from example import class_method, static as custom_static

class Example:
    @static
    def aliased_static():  # no diagnostic
        return 1

    @custom_static
    def custom_static_method():  # no diagnostic
        return 1

    @class_method
    def custom_class_method():  # error: [no-method-argument]
        return 1
```
