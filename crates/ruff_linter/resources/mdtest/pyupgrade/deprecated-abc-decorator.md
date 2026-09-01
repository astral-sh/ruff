# `deprecated-abc-decorator` (`UP051`)

```toml
[lint]
preview = true
select = ["UP051"]
```

## Basic replacements

The more specialized `@abc.abstract*` decorators have been deprecated since Python 3.3 and will
eventually be removed.

```py
import abc

class Foo(abc.ABC):
    @abc.abstractclassmethod  # snapshot: deprecated-abc-decorator
    def class_method(cls, arg1): ...

    @abc.abstractstaticmethod  # snapshot: deprecated-abc-decorator
    def static_method(arg1): ...

    @abc.abstractproperty  # snapshot: deprecated-abc-decorator
    def prop(self): ...
```

```snapshot
error[UP051]: Use `@classmethod` and `@abstractmethod` instead of `abstractclassmethod`
 --> src/mdtest_snippet.py:4:5
  |
4 |     @abc.abstractclassmethod  # snapshot: deprecated-abc-decorator
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `@classmethod` and `abstractmethod`
  |
3 | class Foo(abc.ABC):
  -     @abc.abstractclassmethod  # snapshot: deprecated-abc-decorator
4 +     @classmethod
5 +     @abc.abstractmethod  # snapshot: deprecated-abc-decorator
6 |     def class_method(cls, arg1): ...
  |


error[UP051]: Use `@staticmethod` and `@abstractmethod` instead of `abstractstaticmethod`
 --> src/mdtest_snippet.py:7:5
  |
7 |     @abc.abstractstaticmethod  # snapshot: deprecated-abc-decorator
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `@staticmethod` and `abstractmethod`
  |
6 |
  -     @abc.abstractstaticmethod  # snapshot: deprecated-abc-decorator
7 +     @staticmethod
8 +     @abc.abstractmethod  # snapshot: deprecated-abc-decorator
9 |     def static_method(arg1): ...
  |


error[UP051]: Use `@property` and `@abstractmethod` instead of `abstractproperty`
  --> src/mdtest_snippet.py:10:5
   |
10 |     @abc.abstractproperty  # snapshot: deprecated-abc-decorator
   |     ^^^^^^^^^^^^^^^^^^^^^
help: Replace with `@property` and `abstractmethod`
   |
9  |
   -     @abc.abstractproperty  # snapshot: deprecated-abc-decorator
10 +     @property
11 +     @abc.abstractmethod  # snapshot: deprecated-abc-decorator
12 |     def prop(self): ...
   |
```
