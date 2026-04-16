# Methods

## Background: Functions as descriptors

> Note: See also this related section in the descriptor guide: [Functions and methods].

Say we have a simple class `C` with a function definition `f` inside its body:

```py
class C:
    def f(self, x: int) -> str:
        return "a"
```

Whenever we access the `f` attribute through the class object itself (`C.f`) or through an instance
(`C().f`), this access happens via the descriptor protocol. Functions are (non-data) descriptors
because they implement a `__get__` method. This is crucial in making sure that method calls work as
expected. In general, the signature of the `__get__` method in the descriptor protocol is
`__get__(self, instance, owner)`. The `self` argument is the descriptor object itself (`f`). The
passed value for the `instance` argument depends on whether the attribute is accessed from the class
object (in which case it is `None`), or from an instance (in which case it is the instance of type
`C`). The `owner` argument is the class itself (`C` of type `Literal[C]`). To summarize:

- `C.f` is equivalent to `getattr_static(C, "f").__get__(None, C)`
- `C().f` is equivalent to `getattr_static(C, "f").__get__(C(), C)`

Here, `inspect.getattr_static` is used to bypass the descriptor protocol and directly access the
function attribute. The way the special `__get__` method *on functions* works is as follows. In the
former case, if the `instance` argument is `None`, `__get__` simply returns the function itself. In
the latter case, it returns a *bound method* object:

```py
from inspect import getattr_static

reveal_type(getattr_static(C, "f"))  # revealed: def f(self, x: int) -> str

# revealed: <method-wrapper '__get__' of function 'f'>
reveal_type(getattr_static(C, "f").__get__)

reveal_type(getattr_static(C, "f").__get__(None, C))  # revealed: def f(self, x: int) -> str
reveal_type(getattr_static(C, "f").__get__(C(), C))  # revealed: bound method C.f(x: int) -> str
```

In conclusion, this is why we see the following two types when accessing the `f` attribute on the
class object `C` and on an instance `C()`:

```py
reveal_type(C.f)  # revealed: def f(self, x: int) -> str
reveal_type(C().f)  # revealed: bound method C.f(x: int) -> str
```

A bound method is a callable object that contains a reference to the `instance` that it was called
on (can be inspected via `__self__`), and the function object that it refers to (can be inspected
via `__func__`):

```py
bound_method = C().f

reveal_type(bound_method.__self__)  # revealed: C
reveal_type(bound_method.__func__)  # revealed: def f(self, x: int) -> str
```

When we call the bound method, the `instance` is implicitly passed as the first argument (`self`):

```py
reveal_type(C().f(1))  # revealed: str
reveal_type(bound_method(1))  # revealed: str
```

When we call the function object itself, we need to pass the `instance` explicitly:

```py
# error: [invalid-argument-type] "Argument to function `C.f` is incorrect: Expected `C`, found `Literal[1]`"
# error: [missing-argument]
C.f(1)

reveal_type(C.f(C(), 1))  # revealed: str
```

When we access methods from derived classes, they will be bound to instances of the derived class:

```py
class D(C):
    pass

reveal_type(D().f)  # revealed: bound method D.f(x: int) -> str
```

If we access an attribute on a bound method object itself, it will defer to `types.MethodType`:

```py
reveal_type(bound_method.__hash__)  # revealed: bound method MethodType.__hash__() -> int
```

If an attribute is not available on the bound method object, it will be looked up on the underlying
function object. We model this explicitly, which means that we can access `__kwdefaults__` on bound
methods, even though it is not available on `types.MethodType`:

```py
reveal_type(bound_method.__kwdefaults__)  # revealed: dict[str, Any] | None
```

## Basic method calls on class objects and instances

```py
class Base:
    def method_on_base(self, x: int | None) -> str:
        return "a"

class Derived(Base):
    def method_on_derived(self, x: bytes) -> tuple[int, str]:
        return (1, "a")

reveal_type(Base().method_on_base(1))  # revealed: str
reveal_type(Base.method_on_base(Base(), 1))  # revealed: str

Base().method_on_base("incorrect")  # error: [invalid-argument-type]
Base().method_on_base()  # error: [missing-argument]
Base().method_on_base(1, 2)  # error: [too-many-positional-arguments]

reveal_type(Derived().method_on_base(1))  # revealed: str
reveal_type(Derived().method_on_derived(b"abc"))  # revealed: tuple[int, str]
reveal_type(Derived.method_on_base(Derived(), 1))  # revealed: str
reveal_type(Derived.method_on_derived(Derived(), b"abc"))  # revealed: tuple[int, str]
```

## Method calls on literals

### Boolean literals

```py
reveal_type(True.bit_length())  # revealed: int
reveal_type(True.as_integer_ratio())  # revealed: tuple[int, Literal[1]]
```

### Integer literals

```py
reveal_type((42).bit_length())  # revealed: int
```

### String literals

```py
reveal_type("abcde".find("abc"))  # revealed: int
reveal_type("foo".encode(encoding="utf-8"))  # revealed: bytes

"abcde".find(123)  # error: [invalid-argument-type]
```

### Bytes literals

```py
reveal_type(b"abcde".startswith(b"abc"))  # revealed: bool
```

## Method calls on `LiteralString`

```py
from typing_extensions import LiteralString

def f(s: LiteralString) -> None:
    reveal_type(s.find("a"))  # revealed: int
```

## Method calls on `tuple`

```py
def f(t: tuple[int, str]) -> None:
    reveal_type(t.index("a"))  # revealed: int
```

## Method calls on unions

```py
from typing import Any

class A:
    def f(self) -> int:
        return 1

class B:
    def f(self) -> str:
        return "a"

def f(a_or_b: A | B, any_or_a: Any | A):
    reveal_type(a_or_b.f)  # revealed: (bound method A.f() -> int) | (bound method B.f() -> str)
    reveal_type(a_or_b.f())  # revealed: int | str

    reveal_type(any_or_a.f)  # revealed: Any | (bound method A.f() -> int)
    reveal_type(any_or_a.f())  # revealed: Any | int
```

## Method calls on `KnownInstance` types

```toml
[environment]
python-version = "3.12"
```

```py
type IntOrStr = int | str

reveal_type(IntOrStr.__or__)  # revealed: bound method TypeAliasType.__or__(right: Any, /) -> _SpecialForm
```

## Method calls on types not disjoint from `None`

Very few methods are defined on `object`, `None`, and other types not disjoint from `None`. However,
descriptor-binding behavior works on these types in exactly the same way as descriptor binding on
other types. This is despite the fact that `None` is used as a sentinel internally by the descriptor
protocol to indicate that a method was accessed on the class itself rather than an instance of the
class:

```py
from typing import Protocol, Literal
from ty_extensions import AlwaysFalsy

class Foo: ...

class SupportsStr(Protocol):
    def __str__(self) -> str: ...

class Falsy(Protocol):
    def __bool__(self) -> Literal[False]: ...

def _(a: object, b: SupportsStr, c: Falsy, d: AlwaysFalsy, e: None, f: Foo | None):
    a.__str__()
    b.__str__()
    c.__str__()
    d.__str__()
    e.__str__()
    f.__str__()
```

## Method calls on subclasses of `Any`

```py
from typing_extensions import assert_type, Any

class SubclassOfAny(Any):
    def method(self) -> int:
        return 1

a = SubclassOfAny()
assert_type(a.method(), int)

assert_type(a.non_existing_method(), Any)
```

## Error cases: Calling `__get__` for methods

The `__get__` method on `types.FunctionType` has the following overloaded signature in typeshed:

```pyi
from types import FunctionType, MethodType
from typing import overload

@overload
def __get__(self, instance: None, owner: type, /) -> FunctionType: ...
@overload
def __get__(self, instance: object, owner: type | None = None, /) -> MethodType: ...
```

Here, we test that this signature is enforced correctly:

```py
from inspect import getattr_static

class C:
    def f(self, x: int) -> str:
        return "a"

method_wrapper = getattr_static(C, "f").__get__

reveal_type(method_wrapper)  # revealed: <method-wrapper '__get__' of function 'f'>

# All of these are fine:
method_wrapper(C(), C)
method_wrapper(C())
method_wrapper(C(), None)
method_wrapper(None, C)

reveal_type(object.__str__.__get__(object(), None)())  # revealed: str

# TODO: passing `None` without an `owner` argument fails at runtime.
# Ideally we would emit a diagnostic here:
method_wrapper(None)

# Passing something that is not assignable to `type` as the `owner` argument is an
# error: [no-matching-overload] "No overload of method wrapper `__get__` of function `f` matches arguments"
method_wrapper(None, 1)

# TODO: passing `None` as the `owner` argument when `instance` is `None` fails at runtime.
# Ideally we would emit a diagnostic here.
method_wrapper(None, None)

# Calling `__get__` without any arguments is an
# error: [no-matching-overload] "No overload of method wrapper `__get__` of function `f` matches arguments"
method_wrapper()

# Calling `__get__` with too many positional arguments is an
# error: [no-matching-overload] "No overload of method wrapper `__get__` of function `f` matches arguments"
method_wrapper(C(), C, "one too many")
```

## Fallback to metaclass

When a method is accessed on a class object, it is looked up on the metaclass if it is not found on
the class itself. This also creates a bound method that is bound to the class object itself:

```py
from __future__ import annotations

class Meta(type):
    def f(cls, arg: int) -> str:
        return "a"

class C(metaclass=Meta):
    pass

reveal_type(C.f)  # revealed: bound method <class 'C'>.f(arg: int) -> str
reveal_type(C.f(1))  # revealed: str
```

The method `f` cannot be accessed from an instance of the class:

```py
# error: [unresolved-attribute] "Object of type `C` has no attribute `f`"
C().f
```

A metaclass function can be shadowed by a method on the class:

```py
from typing import Any, Literal

class D(metaclass=Meta):
    def f(arg: int) -> Literal["a"]:
        return "a"

reveal_type(D.f(1))  # revealed: Literal["a"]
```

If the class method is possibly missing, we union the return types:

```py
def flag() -> bool:
    return True

class E(metaclass=Meta):
    if flag():
        def f(arg: int) -> Any:
            return "a"

reveal_type(E.f(1))  # revealed: str | Any
```

## `@classmethod`

### Basic

When a `@classmethod` attribute is accessed, it returns a bound method object, even when accessed on
the class object itself:

```py
from __future__ import annotations

class C:
    @classmethod
    def f(cls: type[C], x: int) -> str:
        return "a"

reveal_type(C.f)  # revealed: bound method <class 'C'>.f(x: int) -> str
reveal_type(C().f)  # revealed: bound method type[C].f(x: int) -> str
```

The `cls` method argument is then implicitly passed as the first argument when calling the method:

```py
reveal_type(C.f(1))  # revealed: str
reveal_type(C().f(1))  # revealed: str
```

When the class method is called incorrectly, we detect it:

```py
C.f("incorrect")  # error: [invalid-argument-type]
C.f()  # error: [missing-argument]
C.f(1, 2)  # error: [too-many-positional-arguments]
```

If the `cls` parameter is wrongly annotated, we emit an error at the call site:

```py
class D:
    @classmethod
    def f(cls: D):
        # This function is wrongly annotated, it should be `type[D]` instead of `D`
        pass

# error: [invalid-argument-type] "Argument to bound method `D.f` is incorrect: Expected `D`, found `<class 'D'>`"
D.f()
```

When a class method is accessed on a derived class, it is bound to that derived class:

```py
class Derived(C):
    pass

reveal_type(Derived.f)  # revealed: bound method <class 'Derived'>.f(x: int) -> str
reveal_type(Derived().f)  # revealed: bound method type[Derived].f(x: int) -> str

reveal_type(Derived.f(1))  # revealed: str
reveal_type(Derived().f(1))  # revealed: str
```

### Accessing the classmethod as a static member

Accessing a `@classmethod`-decorated function at runtime returns a `classmethod` object. We
currently don't model this explicitly:

```py
from inspect import getattr_static

class C:
    @classmethod
    def f(cls): ...

reveal_type(getattr_static(C, "f"))  # revealed: def f(cls) -> Unknown
# revealed: <method-wrapper '__get__' of function 'f'>
reveal_type(getattr_static(C, "f").__get__)
```

But we correctly model how the `classmethod` descriptor works:

```py
reveal_type(getattr_static(C, "f").__get__(None, C))  # revealed: bound method <class 'C'>.f() -> Unknown
reveal_type(getattr_static(C, "f").__get__(C(), C))  # revealed: bound method <class 'C'>.f() -> Unknown
reveal_type(getattr_static(C, "f").__get__(C()))  # revealed: bound method type[C].f() -> Unknown
```

The `owner` argument takes precedence over the `instance` argument:

```py
reveal_type(getattr_static(C, "f").__get__("dummy", C))  # revealed: bound method <class 'C'>.f() -> Unknown
```

### Classmethods mixed with other decorators

```toml
[environment]
python-version = "3.12"
```

When a `@classmethod` is additionally decorated with another decorator, it is still treated as a
class method:

```py
def does_nothing[T](f: T) -> T:
    return f

class C:
    @classmethod
    @does_nothing
    def f1(cls, x: int) -> str:
        return "a"

    @does_nothing
    @classmethod
    def f2(cls, x: int) -> str:
        return "a"

reveal_type(C.f1(1))  # revealed: str
reveal_type(C().f1(1))  # revealed: str
reveal_type(C.f2(1))  # revealed: str
reveal_type(C().f2(1))  # revealed: str
```

### Classmethods with `Self` and callable-returning decorators

When a classmethod is decorated with a decorator that returns a callable type (like
`@contextmanager`), `Self` in the return type should correctly resolve to the subclass when accessed
on a derived class.

```py
from contextlib import contextmanager
from typing import Iterator
from typing_extensions import Self

class Base:
    @classmethod
    @contextmanager
    def create(cls) -> Iterator[Self]:
        yield cls()

class Child(Base): ...

reveal_type(Base.create())  # revealed: _GeneratorContextManager[Base, None, None]
with Base.create() as base:
    reveal_type(base)  # revealed: Base

reveal_type(Base().create())  # revealed: _GeneratorContextManager[Base, None, None]
with Base().create() as base:
    reveal_type(base)  # revealed: Base

reveal_type(Child.create())  # revealed: _GeneratorContextManager[Child, None, None]
with Child.create() as child:
    reveal_type(child)  # revealed: Child

reveal_type(Child().create())  # revealed: _GeneratorContextManager[Child, None, None]
with Child().create() as child:
    reveal_type(child)  # revealed: Child
```

### `__init_subclass__`

```toml
[environment]
python-version = "3.12"
```

The [`__init_subclass__`] method is implicitly a classmethod:

```py
class Base:
    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        cls.custom_attribute: int = 0

class Derived(Base):
    pass

reveal_type(Derived.custom_attribute)  # revealed: int
```

Subclasses must be constructed with arguments matching the required arguments of the base
`__init_subclass__` method.

```py
class Empty: ...

class RequiresArg:
    def __init_subclass__(cls, arg: int): ...

class NoArg:
    def __init_subclass__(cls): ...

# Single-base definitions
class MissingArg(RequiresArg): ...  # error: [missing-argument]
class InvalidType(RequiresArg, arg="foo"): ...  # error: [invalid-argument-type]
class Valid(RequiresArg, arg=1): ...

# snapshot: missing-argument
# snapshot: unknown-argument
class IncorrectArg(RequiresArg, not_arg="foo"):
    a = 1
    b = 2
    c = 3
    d = 4
    e = 5
    f = 6
    g = 7
    h = 8
    i = 9
    j = 10
```

```snapshot
error[missing-argument]: No argument provided for required parameter `arg` of function `RequiresArg.__init_subclass__`
  --> src/mdtest_snippet.py:25:1
   |
25 | class IncorrectArg(RequiresArg, not_arg="foo"):
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
  --> src/mdtest_snippet.py:13:32
   |
13 |     def __init_subclass__(cls, arg: int): ...
   |                                ^^^^^^^^
   |


error[unknown-argument]: Argument `not_arg` does not match any known parameter of function `RequiresArg.__init_subclass__`
  --> src/mdtest_snippet.py:25:33
   |
25 | class IncorrectArg(RequiresArg, not_arg="foo"):
   |                                 ^^^^^^^^^^^^^
   |
info: Function signature here
  --> src/mdtest_snippet.py:13:9
   |
13 |     def __init_subclass__(cls, arg: int): ...
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
```

```py
class NotCallableInitSubclass:
    __init_subclass__ = None

# snapshot
class Bad(NotCallableInitSubclass):
    a = 1
    b = 2
    c = 3
```

```snapshot
error[non-callable-init-subclass]: Invalid definition of class `Bad`
  --> src/mdtest_snippet.py:40:7
   |
40 | class Bad(NotCallableInitSubclass):
   |       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Superclass `NotCallableInitSubclass` cannot be subclassed
   |
  ::: src/mdtest_snippet.py:37:5
   |
37 |     __init_subclass__ = None
   |     ----------------- `NotCallableInitSubclass.__init_subclass__` has type `None | Unknown`, which may not be callable
   |
info: `__init_subclass__` on a superclass is implicitly called during creation of a class object
info: See https://docs.python.org/3/reference/datamodel.html#customizing-class-creation
```

The `metaclass` keyword is ignored, as it has special meaning and is not passed to
`__init_subclass__` at runtime.

```py
class Base:
    def __init_subclass__(cls, arg: int): ...

class Valid(Base, arg=5, metaclass=object): ...

# Despite the explicit `metaclass=object` call,
# the metaclass of this class is `type`, because that is
# the metaclass of its superclass `object`, and `type`
# (metaclass of superclass) is a subclass of `object`
# (explicit `metaclass=` argument in this class statement).
reveal_type(Valid.__class__)  # revealed: <class 'type'>

# error: [invalid-argument-type] "Argument to function `Base.__init_subclass__` is incorrect: Expected `int`, found `Literal["foo"]`"
class Invalid(Base, metaclass=type, arg="foo"): ...
```

Overload matching is performed correctly:

```py
from typing import Literal, overload

class Base:
    @overload
    def __init_subclass__(cls, mode: Literal["a"], arg: int) -> None: ...
    @overload
    def __init_subclass__(cls, mode: Literal["b"], arg: str) -> None: ...
    def __init_subclass__(cls, mode: str, arg: int | str) -> None: ...

class Valid(Base, mode="a", arg=5): ...
class Valid(Base, mode="b", arg="foo"): ...

# snapshot: no-matching-overload
class InvalidType(Base, mode="b", arg=5):
    a = 1
    b = 2
    c = 3
    d = 4
    e = 5
```

```snapshot
error[no-matching-overload]: No overload of function `Base.__init_subclass__` matches arguments
  --> src/mdtest_snippet.py:71:1
   |
71 | class InvalidType(Base, mode="b", arg=5):
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: First overload defined here
  --> src/mdtest_snippet.py:62:9
   |
62 |     def __init_subclass__(cls, mode: Literal["a"], arg: int) -> None: ...
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Possible overloads for function `__init_subclass__`:
info:   (cls, mode: Literal["a"], arg: int) -> None
info:   (cls, mode: Literal["b"], arg: str) -> None
info: Overload implementation defined here
  --> src/mdtest_snippet.py:65:9
   |
65 |     def __init_subclass__(cls, mode: str, arg: int | str) -> None: ...
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
```

For multiple inheritance, the first resolved `__init_subclass__` method is used.

```py
class Empty: ...

class RequiresArg:
    def __init_subclass__(cls, arg: int): ...

class NoArg:
    def __init_subclass__(cls): ...

class Valid(NoArg, RequiresArg): ...
class MissingArg(RequiresArg, NoArg): ...  # error: [missing-argument]
class InvalidType(RequiresArg, NoArg, arg="foo"): ...  # error: [invalid-argument-type]
class Valid(RequiresArg, NoArg, arg=1): ...

# Ensure base class without __init_subclass__ is ignored
class Valid(Empty, NoArg): ...
class Valid(Empty, RequiresArg, NoArg, arg=1): ...
class MissingArg(Empty, RequiresArg): ...  # error: [missing-argument]
class MissingArg(Empty, RequiresArg, NoArg): ...  # error: [missing-argument]
class InvalidType(Empty, RequiresArg, NoArg, arg="foo"): ...  # error: [invalid-argument-type]

# Multiple inheritance with args
class Base(Empty, RequiresArg, NoArg, arg=1): ...
class Valid(Base, arg=1): ...
class MissingArg(Base): ...  # error: [missing-argument]
class InvalidType(Base, arg="foo"): ...  # error: [invalid-argument-type]
```

Keyword splats are allowed if their type can be determined:

```py
from typing import TypedDict

class RequiresKwarg:
    def __init_subclass__(cls, arg: int): ...

class WrongArg(TypedDict):
    kwarg: int

class InvalidType(TypedDict):
    arg: str

wrong_arg: WrongArg = {"kwarg": 5}

# error: [missing-argument]
# error: [unknown-argument]
class MissingArg(RequiresKwarg, **wrong_arg): ...

invalid_type: InvalidType = {"arg": "foo"}

# error: [invalid-argument-type]
class InvalidType(RequiresKwarg, **invalid_type): ...
```

So are generics:

```py
from typing import Generic, TypeVar, Literal, overload

class Base[T]:
    def __init_subclass__(cls, arg: T): ...

class Valid(Base[int], arg=1): ...
class InvalidType(Base[int], arg="x"): ...  # error: [invalid-argument-type]

# Old generic syntax
T = TypeVar("T")

class Base(Generic[T]):
    def __init_subclass__(cls, arg: T) -> None: ...

class Valid(Base[int], arg=1): ...
class InvalidType(Base[int], arg="x"): ...  # error: [invalid-argument-type]
```

### Metaclass `__new__` keyword arguments

When a custom metaclass overrides `__new__` with keyword-only parameters, class keyword arguments
are validated against the metaclass's `__new__` signature instead of `__init_subclass__`.

```py
from typing import Any

class Meta(type):
    def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], *, required_arg: int):
        return super().__new__(mcs, name, bases, namespace)

class Valid(metaclass=Meta, required_arg=5): ...
class MissingArg(metaclass=Meta): ...  # snapshot: missing-argument
class InvalidType(metaclass=Meta, required_arg="foo"): ...  # snapshot: invalid-argument-type
```

```snapshot
error[missing-argument]: No argument provided for required parameter `required_arg` of constructor `Meta.__new__`
 --> src/mdtest_snippet.py:8:1
  |
8 | class MissingArg(metaclass=Meta): ...  # snapshot: missing-argument
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
info: Parameter declared here
 --> src/mdtest_snippet.py:4:88
  |
4 |     def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], *, required_arg: int):
  |                                                                                        ^^^^^^^^^^^^^^^^^
  |


error[invalid-argument-type]: Argument to constructor `Meta.__new__` is incorrect
 --> src/mdtest_snippet.py:9:35
  |
9 | class InvalidType(metaclass=Meta, required_arg="foo"): ...  # snapshot: invalid-argument-type
  |                                   ^^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["foo"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:4:9
  |
4 |     def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], *, required_arg: int):
  |         ^^^^^^^                                                                        ----------------- Parameter declared here
  |
```

### Metaclass `__new__` takes priority over `__init_subclass__`

If a metaclass defines `__new__`, we do no checking of a superclass `__init_subclass__` method even
if one exists on a base. `__new__` on the metaclass could consume or add arbitrary keyword arguments
before passing them onto `__init_subclass__`, so there is no way for us to check that
`__init_subclass__` has been called correctly in this scenario:

```py
from typing import Any

class Meta(type):
    def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], *, meta_arg: int):
        return super().__new__(mcs, name, bases, namespace, sub_arg="ooh, fancy")

class Base:
    def __init_subclass__(cls, sub_arg: str): ...

# `meta_arg` is checked against `Meta.__new__`, not `Base.__init_subclass__`
class Valid(Base, meta_arg=5, metaclass=Meta): ...
class MissingArg(Base, metaclass=Meta): ...  # snapshot: missing-argument
class InvalidType(Base, meta_arg="foo", metaclass=Meta): ...  # snapshot: invalid-argument-type

# snapshot: missing-argument
class Invalid2(metaclass=Meta):
    def __init_subclass__(cls, sub_arg: str): ...
```

```snapshot
error[missing-argument]: No argument provided for required parameter `meta_arg` of constructor `Meta.__new__`
  --> src/mdtest_snippet.py:12:1
   |
12 | class MissingArg(Base, metaclass=Meta): ...  # snapshot: missing-argument
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
 --> src/mdtest_snippet.py:4:88
  |
4 |     def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], *, meta_arg: int):
  |                                                                                        ^^^^^^^^^^^^^
  |


error[invalid-argument-type]: Argument to constructor `Meta.__new__` is incorrect
  --> src/mdtest_snippet.py:13:25
   |
13 | class InvalidType(Base, meta_arg="foo", metaclass=Meta): ...  # snapshot: invalid-argument-type
   |                         ^^^^^^^^^^^^^^ Expected `int`, found `Literal["foo"]`
   |
info: Function defined here
 --> src/mdtest_snippet.py:4:9
  |
4 |     def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], *, meta_arg: int):
  |         ^^^^^^^                                                                        ------------- Parameter declared here
  |


error[missing-argument]: No argument provided for required parameter `meta_arg` of constructor `Meta.__new__`
  --> src/mdtest_snippet.py:16:1
   |
16 | class Invalid2(metaclass=Meta):
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
 --> src/mdtest_snippet.py:4:88
  |
4 |     def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], *, meta_arg: int):
  |                                                                                        ^^^^^^^^^^^^^
  |
```

### Metaclass `__prepare__`

When a metaclass defines `__prepare__`, class keyword arguments are also validated against its
signature.

```py
from typing import Any

class Meta(type):
    @classmethod
    def __prepare__(mcs, name: str, bases: tuple[type, ...], *, prep_arg: int = 0, **kwargs: Any) -> dict[str, Any]:
        return {}

    def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], **kwargs: Any):
        return super().__new__(mcs, name, bases, namespace)

class Valid(metaclass=Meta, prep_arg=5): ...
class InvalidType(metaclass=Meta, prep_arg="foo"): ...  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to bound method `Meta.__prepare__` is incorrect
  --> src/mdtest_snippet.py:12:35
   |
12 | class InvalidType(metaclass=Meta, prep_arg="foo"): ...  # snapshot: invalid-argument-type
   |                                   ^^^^^^^^^^^^^^ Expected `int`, found `Literal["foo"]`
   |
info: Method defined here
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __prepare__(mcs, name: str, bases: tuple[type, ...], *, prep_arg: int = 0, **kwargs: Any) -> dict[str, Any]:
  |         ^^^^^^^^^^^                                             ----------------- Parameter declared here
  |
```

When `__new__` expects a custom `dict` subclass as the namespace parameter, and `__prepare__`
returns a plain `dict[str, Any]`, this should produce a type error on the namespace argument.

```py
from typing import Any

class MyNamespace(dict[str, Any]):
    pass

class Meta(type):
    @classmethod
    def __prepare__(mcs, name: str, bases: tuple[type, ...], **kwargs: Any) -> dict[str, Any]:
        return {}

    def __new__(mcs, name: str, bases: tuple[type, ...], namespace: MyNamespace, **kwargs: Any):
        return super().__new__(mcs, name, bases, namespace)

class Foo(metaclass=Meta): ...  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to constructor `Meta.__new__` is incorrect
  --> src/mdtest_snippet.py:26:10
   |
26 | class Foo(metaclass=Meta): ...  # snapshot: invalid-argument-type
   |          ^^^^^^^^^^^^^^^^ Expected `MyNamespace`, found `dict[str, Any]`
   |
info: Function defined here
  --> src/mdtest_snippet.py:23:9
   |
23 |     def __new__(mcs, name: str, bases: tuple[type, ...], namespace: MyNamespace, **kwargs: Any):
   |         ^^^^^^^                                          ---------------------- Parameter declared here
   |
```

When `__prepare__` returns the expected custom namespace type, no error should be emitted.

```py
from typing import Any

class Meta(type):
    @classmethod
    def __prepare__(mcs, name: str, bases: tuple[type, ...], **kwargs: Any) -> MyNamespace:
        return MyNamespace()

    def __new__(mcs, name: str, bases: tuple[type, ...], namespace: MyNamespace, **kwargs: Any):
        return super().__new__(mcs, name, bases, namespace)

class Foo(metaclass=Meta): ...
```

We complain if `__prepare__` is set to a non-callable type such as `None`, and point to the binding
of `__prepare__` in our diagnostic:

```py
class Meta(type):
    __prepare__ = None

# snapshot: invalid-metaclass
class Foo(metaclass=Meta): ...
class MetaSub(Meta): ...

# error: [invalid-metaclass]
class Bar(Foo, metaclass=MetaSub): ...
```

```snapshot
error[invalid-metaclass]: Invalid definition of class `Foo`
  --> src/mdtest_snippet.py:42:11
   |
42 | class Foo(metaclass=Meta): ...
   |           ^^^^^^^^^^^^^^
   |           |
   |           Class creation will fail at runtime due to its metaclass
   |           Metaclass `<class 'Meta'>` has an invalid `__prepare__` definition
   |
  ::: src/mdtest_snippet.py:39:5
   |
39 |     __prepare__ = None
   |     ----------- Metaclass `__prepare__` defined here
   |
info: `__prepare__` on a class's metaclass is implicitly called during creation of the class object
info: See https://docs.python.org/3/reference/datamodel.html#preparing-the-class-namespace
```

### Metaclass `__new__`, `__init__` and `__prepare__` all defined

```py
from typing import Any

class Meta(type):
    @classmethod
    def __prepare__(  # error: [invalid-method-override]
        mcs, name: str, bases: tuple[type, ...], prepare_arg: int, **kwargs: Any
    ) -> dict[str, Any]:
        return {}

    def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], dunder_new_arg: int, **kwargs: Any):
        return super().__new__(mcs, name, bases, namespace)

    # TODO: we could complain here that `__init__` has an incompatible signature with `__new__`
    def __init__(cls, *args, init_arg: int): ...

# Implicit calls to metaclass constructors work the same way as explicit "regular" constructor calls.
# We do not complain about the missing argument to the metaclass `__init__` method here:
# it'll never get called because of the missing argument to the metaclass `__new__` method
#
# snapshot: missing-argument
# snapshot: missing-argument
class Foo(metaclass=Meta): ...
```

```snapshot
error[missing-argument]: No argument provided for required parameter `prepare_arg` of bound method `Meta.__prepare__`
  --> src/mdtest_snippet.py:22:1
   |
22 | class Foo(metaclass=Meta): ...
   | ^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
 --> src/mdtest_snippet.py:6:50
  |
6 |         mcs, name: str, bases: tuple[type, ...], prepare_arg: int, **kwargs: Any
  |                                                  ^^^^^^^^^^^^^^^^
  |


error[missing-argument]: No argument provided for required parameter `dunder_new_arg` of constructor `Meta.__new__`
  --> src/mdtest_snippet.py:22:1
   |
22 | class Foo(metaclass=Meta): ...
   | ^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
  --> src/mdtest_snippet.py:10:85
   |
10 |     def __new__(mcs, name: str, bases: tuple[type, ...], namespace: dict[str, Any], dunder_new_arg: int, **kwargs: Any):
   |                                                                                     ^^^^^^^^^^^^^^^^^^^
   |
```

### Metaclass `__new__` expects a fixed-length tuple of bases

```py
from typing import Any

class Meta(type):
    def __new__(metacls, name: str, bases: tuple[type, type], ns: dict[str, Any]): ...

class A: ...
class B: ...

# This errors because a `bases` tuple of length 0 was passed to metaclass `__new__`
#
# snapshot: invalid-argument-type
class Bad1(metaclass=Meta): ...
```

```snapshot
error[invalid-argument-type]: Argument to constructor `Meta.__new__` is incorrect
  --> src/mdtest_snippet.py:12:11
   |
12 | class Bad1(metaclass=Meta): ...
   |           ^^^^^^^^^^^^^^^^ Expected `tuple[type, type]`, found `tuple[()]`
   |
info: Function defined here
 --> src/mdtest_snippet.py:4:9
  |
4 |     def __new__(metacls, name: str, bases: tuple[type, type], ns: dict[str, Any]): ...
  |         ^^^^^^^                     ------------------------ Parameter declared here
  |
```

```py
# This errors because a `bases` tuple of length 1 was passed to metaclass `__new__`
#
# snapshot: invalid-argument-type
class Bad2(A, metaclass=Meta): ...
```

```snapshot
error[invalid-argument-type]: Argument to constructor `Meta.__new__` is incorrect
  --> src/mdtest_snippet.py:16:11
   |
16 | class Bad2(A, metaclass=Meta): ...
   |           ^^^^^^^^^^^^^^^^^^^ Expected `tuple[type, type]`, found `tuple[<class 'A'>]`
   |
info: Function defined here
 --> src/mdtest_snippet.py:4:9
  |
4 |     def __new__(metacls, name: str, bases: tuple[type, type], ns: dict[str, Any]): ...
  |         ^^^^^^^                     ------------------------ Parameter declared here
  |
```

```py
# This succeeds
class Good(A, B, metaclass=Meta): ...
```

### Metaclass `__new__` expects a string-literal type

```py
from typing import Literal, Any

class Meta(type):
    def __new__(metacls, name: Literal["Foo"], bases: tuple[type, ...], namespace: dict[str, Any]): ...

# this errors because `Literal["Bad"]` was passed as the `name` argument to `Meta.__new__`,
# instead of `Literal["Foo"]`
#
# error: [invalid-argument-type]
class Bad(metaclass=Meta): ...
class Foo(metaclass=Meta): ...
```

### If a metaclass does not override `__new__`, we still check `__init_subclass__`

The mere presence of a custom metaclass does not prevent us from checking `__init_subclass__`: if
the metaclass is a `type` subclass that does not override `__new__`, we will still do so:

```py
class Meta(type):
    pass

class Base(metaclass=Meta):
    def __init_subclass__(cls, arg: int): ...

class Valid(Base, arg=5): ...
class MissingArg(Base): ...  # snapshot: missing-argument
class InvalidType(Base, arg="foo"): ...  # snapshot: invalid-argument-type
```

```snapshot
error[missing-argument]: No argument provided for required parameter `arg` of function `Base.__init_subclass__`
 --> src/mdtest_snippet.py:8:1
  |
8 | class MissingArg(Base): ...  # snapshot: missing-argument
  | ^^^^^^^^^^^^^^^^^^^^^^
  |
info: Parameter declared here
 --> src/mdtest_snippet.py:5:32
  |
5 |     def __init_subclass__(cls, arg: int): ...
  |                                ^^^^^^^^
  |


error[invalid-argument-type]: Argument to function `Base.__init_subclass__` is incorrect
 --> src/mdtest_snippet.py:9:25
  |
9 | class InvalidType(Base, arg="foo"): ...  # snapshot: invalid-argument-type
  |                         ^^^^^^^^^ Expected `int`, found `Literal["foo"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __init_subclass__(cls, arg: int): ...
  |         ^^^^^^^^^^^^^^^^^      -------- Parameter declared here
  |
```

This includes if the metaclass defines `__init__`: `__init_subclass__` is called before the
metaclass's `__init__` method, so it is not possible for a metaclass's `__init__` method to consume
or inject arguments before `__init_subclass__` is called in the same way that it is possible for a
metaclass's `__new__` method:

```py
class Meta2(type):
    def __init__(cls, *args, **kwargs): ...

class Base(metaclass=Meta2):
    def __init_subclass__(cls, arg: int): ...

class Valid(Base, arg=5): ...
class MissingArg(Base): ...  # snapshot: missing-argument
class InvalidType(Base, arg="foo"): ...  # snapshot: invalid-argument-type
```

```snapshot
error[missing-argument]: No argument provided for required parameter `arg` of function `Base.__init_subclass__`
  --> src/mdtest_snippet.py:17:1
   |
17 | class MissingArg(Base): ...  # snapshot: missing-argument
   | ^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
  --> src/mdtest_snippet.py:14:32
   |
14 |     def __init_subclass__(cls, arg: int): ...
   |                                ^^^^^^^^
   |


error[invalid-argument-type]: Argument to function `Base.__init_subclass__` is incorrect
  --> src/mdtest_snippet.py:18:25
   |
18 | class InvalidType(Base, arg="foo"): ...  # snapshot: invalid-argument-type
   |                         ^^^^^^^^^ Expected `int`, found `Literal["foo"]`
   |
info: Function defined here
  --> src/mdtest_snippet.py:14:9
   |
14 |     def __init_subclass__(cls, arg: int): ...
   |         ^^^^^^^^^^^^^^^^^      -------- Parameter declared here
   |
```

```py
class MetaclassWithFancyInit(type):
    def __init__(cls, *args, metaclass_arg: int, **kwargs): ...

# `object.__init_subclass__` is called before `MetaclassWithFancyInit.__init__` here, so this is an...
#
# error: [unknown-argument] "Argument `metaclass_arg` does not match any known parameter of function `object.__init_subclass__`"
class Base2(metaclass=MetaclassWithFancyInit, metaclass_arg=42):
    def __init_subclass__(cls, init_subclass_arg: int): ...

class Base3:
    def __init_subclass__(cls, *args, **kwargs): ...

# In this situation, the permissive signature of `Base3.__init_subclass__` allows
# the `metaclass_arg` argument to be passed unhindered to the metaclass `__init__` method
class Fine(Base3, metaclass=MetaclassWithFancyInit, metaclass_arg=42): ...

# snapshot: missing-argument
class NotGood(Base3, metaclass=MetaclassWithFancyInit): ...
```

```snapshot
error[missing-argument]: No argument provided for required parameter `metaclass_arg` of `MetaclassWithFancyInit.__init__`
  --> src/mdtest_snippet.py:36:1
   |
36 | class NotGood(Base3, metaclass=MetaclassWithFancyInit): ...
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
  --> src/mdtest_snippet.py:20:30
   |
20 |     def __init__(cls, *args, metaclass_arg: int, **kwargs): ...
   |                              ^^^^^^^^^^^^^^^^^^
   |
```

### Metaclass with a custom metaclass

If the metaclass itself has a custom metaclass and the meta-metaclass defines `__call__`, we do call
checking against `__call__` on the meta-metaclass:

```py
class MetaMeta(type):
    def __call__(metacls, *args, meta_meta_arg: int, **kwargs) -> str:
        return "foo"

class Meta(type, metaclass=MetaMeta): ...

# snapshot: missing-argument
class Bad(metaclass=Meta): ...

# TODO: should be `str` because of the return type of `MetaMeta.__call__`
reveal_type(Bad)  # revealed: <class 'Bad'>

class Good(metaclass=Meta, meta_meta_arg=42): ...
```

```snapshot
error[missing-argument]: No argument provided for required parameter `meta_meta_arg` of bound method `MetaMeta.__call__`
 --> src/mdtest_snippet.py:8:1
  |
8 | class Bad(metaclass=Meta): ...
  | ^^^^^^^^^^^^^^^^^^^^^^^^^
  |
info: Parameter declared here
 --> src/mdtest_snippet.py:2:34
  |
2 |     def __call__(metacls, *args, meta_meta_arg: int, **kwargs) -> str:
  |                                  ^^^^^^^^^^^^^^^^^^
  |
```

Similar to the case where a metaclass defines `__new__`, this also prevents us from checking the
call signature of `__init_subclass__`, since the `__call__` method of a meta-metaclass is called
prior to `__init_subclass__` on the class and could therefore consume or inject arbitrary arguments
when the `__init_subclass__` method is called:

```py
class InitSubclassBase(metaclass=Meta, meta_meta_arg=42):
    def __init_subclass__(cls, required_arg: int): ...

class InitSubclassNotCheckedHere(InitSubclassBase, metaclass=Meta, meta_meta_arg=42): ...
```

But if the meta-metaclass does not override `__call__`, `__init_subclass__` is still checked:

```py
class LessFancyMetaMeta(type): ...
class LessFancyMeta(type, metaclass=LessFancyMetaMeta): ...

class WithInitSubclass(metaclass=LessFancyMeta):
    def __init_subclass__(cls, arg: int): ...

# error: [missing-argument] "No argument provided for required parameter `arg` of function `WithInitSubclass.__init_subclass__`"
class Bad(WithInitSubclass): ...
class Good(WithInitSubclass, arg=42): ...
```

### Metaclass with a custom `__new__` method and a custom meta-metaclass

Same as the way that we do not check `__new__` if a regular class's metaclass defines `__call__` and
`__call__` does not return an instance of the class, we also do not check `__new__` on a metaclass
if the meta-metaclass has a `__call__` method that does not return an instance of the metaclass:

```py
class MetaMeta(type):
    def __call__(metacls, *args, meta_meta_arg: int, **kwargs) -> str:
        return "foo"

class Meta(type, metaclass=MetaMeta):
    def __new__(metacls, *args, meta_arg: int, **kwargs): ...

# snapshot: missing-argument
class Bad(metaclass=Meta): ...
```

```snapshot
error[missing-argument]: No argument provided for required parameter `meta_meta_arg` of bound method `MetaMeta.__call__`
 --> src/mdtest_snippet.py:9:1
  |
9 | class Bad(metaclass=Meta): ...
  | ^^^^^^^^^^^^^^^^^^^^^^^^^
  |
info: Parameter declared here
 --> src/mdtest_snippet.py:2:34
  |
2 |     def __call__(metacls, *args, meta_meta_arg: int, **kwargs) -> str:
  |                                  ^^^^^^^^^^^^^^^^^^
  |
```

```py
# `Meta.__new__` is not checked because `MetaMeta.__call__` returns `str`,
# so no diagnostic is emitted here
class Fine(metaclass=Meta, meta_meta_arg=42): ...

class MetaMeta2(type):
    def __call__(metacls, *args, **kwargs) -> "Meta2":
        return type.__call__(metacls, *args, **kwargs)

class Meta2(type, metaclass=MetaMeta2):
    def __new__(metacls, *args, meta_arg: int, **kwargs): ...

# snapshot: missing-argument
class AlsoBad(metaclass=Meta2): ...
```

```snapshot
error[missing-argument]: No argument provided for required parameter `meta_arg` of constructor `Meta2.__new__`
  --> src/mdtest_snippet.py:22:1
   |
22 | class AlsoBad(metaclass=Meta2): ...
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
  --> src/mdtest_snippet.py:19:33
   |
19 |     def __new__(metacls, *args, meta_arg: int, **kwargs): ...
   |                                 ^^^^^^^^^^^^^
   |
```

```py
class MetaMeta3(type):
    def __call__(metacls, *args, **kwargs) -> "Meta3":
        return type.__call__(metacls, *args, **kwargs)

class Meta3(type, metaclass=MetaMeta3):
    def __init__(cls, *args, init_arg: int, **kwargs): ...

# snapshot: missing-argument
class AlsoFine(metaclass=Meta3): ...
```

```snapshot
error[missing-argument]: No argument provided for required parameter `init_arg` of `Meta3.__init__`
  --> src/mdtest_snippet.py:31:1
   |
31 | class AlsoFine(metaclass=Meta3): ...
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
  --> src/mdtest_snippet.py:28:30
   |
28 |     def __init__(cls, *args, init_arg: int, **kwargs): ...
   |                              ^^^^^^^^^^^^^
   |
```

## `@staticmethod`

### Basic

When a `@staticmethod` attribute is accessed, it returns the underlying function object. This is
true whether it's accessed on the class or on an instance of the class.

```py
from __future__ import annotations

class C:
    @staticmethod
    def f(x: int) -> str:
        return "a"

reveal_type(C.f)  # revealed: def f(x: int) -> str
reveal_type(C().f)  # revealed: def f(x: int) -> str
```

The method can then be called like a regular function from either the class or an instance, with no
implicit first argument passed.

```py
reveal_type(C.f(1))  # revealed: str
reveal_type(C().f(1))  # revealed: str
```

When the static method is called incorrectly, we detect it:

```py
C.f("incorrect")  # error: [invalid-argument-type]
C.f()  # error: [missing-argument]
C.f(1, 2)  # error: [too-many-positional-arguments]
```

When a static method is accessed on a derived class, it behaves identically:

```py
class Derived(C):
    pass

reveal_type(Derived.f)  # revealed: def f(x: int) -> str
reveal_type(Derived().f)  # revealed: def f(x: int) -> str

reveal_type(Derived.f(1))  # revealed: str
reveal_type(Derived().f(1))  # revealed: str
```

### Staticmethod assigned in class body

Assigning a `staticmethod(...)` object directly in the class body should preserve the callable
behavior of the wrapped function when accessed on both classes and instances.

```py
def foo(*args, **kwargs) -> None:
    print("foo", args, kwargs)

class A:
    __call__ = staticmethod(foo)
    bar = staticmethod(foo)

a = A()
a()
a.bar()
a(5)
a.bar(5)
a(x=10)
a.bar(x=10)

A.bar()
A.bar(5)
A.bar(x=10)
```

### Accessing the staticmethod as a static member

```py
from inspect import getattr_static

class C:
    @staticmethod
    def f(): ...
```

Accessing the staticmethod as a static member. This will reveal the raw function, as `staticmethod`
is transparent when accessed via `getattr_static`.

```py
reveal_type(getattr_static(C, "f"))  # revealed: def f() -> Unknown
```

The `__get__` of a `staticmethod` object simply returns the underlying function. It ignores both the
instance and owner arguments.

```py
reveal_type(getattr_static(C, "f").__get__(None, C))  # revealed: def f() -> Unknown
reveal_type(getattr_static(C, "f").__get__(C(), C))  # revealed: def f() -> Unknown
reveal_type(getattr_static(C, "f").__get__(C()))  # revealed: def f() -> Unknown
reveal_type(getattr_static(C, "f").__get__("dummy", C))  # revealed: def f() -> Unknown
```

### Staticmethods mixed with other decorators

```toml
[environment]
python-version = "3.12"
```

When a `@staticmethod` is additionally decorated with another decorator, it is still treated as a
static method:

```py
from __future__ import annotations

def does_nothing[T](f: T) -> T:
    return f

class C:
    @staticmethod
    @does_nothing
    def f1(x: int) -> str:
        return "a"

    @does_nothing
    @staticmethod
    def f2(x: int) -> str:
        return "a"

reveal_type(C.f1(1))  # revealed: str
reveal_type(C().f1(1))  # revealed: str
reveal_type(C.f2(1))  # revealed: str
reveal_type(C().f2(1))  # revealed: str
```

When a `@staticmethod` is decorated with `@contextmanager`, accessing it from an instance should not
bind `self`:

```py
from contextlib import contextmanager
from collections.abc import Iterator

class D:
    @staticmethod
    @contextmanager
    def ctx(num: int) -> Iterator[int]:
        yield num

    def use_ctx(self) -> None:
        # Accessing via self should not bind self
        with self.ctx(10) as x:
            reveal_type(x)  # revealed: int

# Accessing via class works
reveal_type(D.ctx(5))  # revealed: _GeneratorContextManager[int, None, None]

# Accessing via instance should also work (no self-binding)
reveal_type(D().ctx(5))  # revealed: _GeneratorContextManager[int, None, None]
```

### `__new__`

`__new__` is an implicit `@staticmethod`; accessing it on an instance does not bind the `cls`
argument:

```py
from typing_extensions import Self

reveal_type(object.__new__)  # revealed: def __new__[Self](cls) -> Self
reveal_type(object().__new__)  # revealed: def __new__[Self](cls) -> Self
# revealed: Overload[[Self](cls, x: str | Buffer | SupportsInt | SupportsIndex | SupportsTrunc = 0, /) -> Self, [Self](cls, x: str | bytes | bytearray, /, base: SupportsIndex) -> Self]
reveal_type(int.__new__)
# revealed: Overload[[Self](cls, x: str | Buffer | SupportsInt | SupportsIndex | SupportsTrunc = 0, /) -> Self, [Self](cls, x: str | bytes | bytearray, /, base: SupportsIndex) -> Self]
reveal_type((42).__new__)

class X:
    def __init__(self, val: int): ...
    def make_another(self) -> Self:
        reveal_type(self.__new__)  # revealed: def __new__[Self](cls) -> Self
        return self.__new__(type(self))
```

## Builtin functions and methods

Some builtin functions and methods are heavily special-cased by ty. This mdtest checks that various
properties are understood correctly for these functions and methods.

```py
import types
from typing import Callable
from ty_extensions import static_assert, RegularCallableTypeOf, is_assignable_to, TypeOf

def f(obj: type) -> None: ...

class MyClass:
    @property
    def my_property(self) -> int:
        return 42

    @my_property.setter
    def my_property(self, value: int | str) -> None: ...

static_assert(is_assignable_to(types.FunctionType, Callable))

# revealed: <wrapper-descriptor '__get__' of 'function' objects>
reveal_type(types.FunctionType.__get__)
static_assert(is_assignable_to(TypeOf[types.FunctionType.__get__], Callable))

# revealed: def f(obj: type) -> None
reveal_type(f)
static_assert(is_assignable_to(TypeOf[f], Callable))

# revealed: <method-wrapper '__get__' of function 'f'>
reveal_type(f.__get__)
static_assert(is_assignable_to(TypeOf[f.__get__], Callable))

# revealed: def __call__(self, *args: Any, **kwargs: Any) -> Any
reveal_type(types.FunctionType.__call__)
static_assert(is_assignable_to(TypeOf[types.FunctionType.__call__], Callable))

# revealed: <method-wrapper '__call__' of function 'f'>
reveal_type(f.__call__)
static_assert(is_assignable_to(TypeOf[f.__call__], Callable))

# revealed: <wrapper-descriptor '__get__' of 'property' objects>
reveal_type(property.__get__)
static_assert(is_assignable_to(TypeOf[property.__get__], Callable))

# revealed: property
reveal_type(MyClass.my_property)
static_assert(is_assignable_to(TypeOf[property], Callable))
static_assert(not is_assignable_to(TypeOf[MyClass.my_property], Callable))

# revealed: <method-wrapper '__get__' of property 'my_property'>
reveal_type(MyClass.my_property.__get__)
static_assert(is_assignable_to(TypeOf[MyClass.my_property.__get__], Callable))

# revealed: <wrapper-descriptor '__set__' of 'property' objects>
reveal_type(property.__set__)
static_assert(is_assignable_to(TypeOf[property.__set__], Callable))

# revealed: <method-wrapper '__set__' of property 'my_property'>
reveal_type(MyClass.my_property.__set__)
static_assert(is_assignable_to(TypeOf[MyClass.my_property.__set__], Callable))

# revealed: def startswith(self, prefix: str | tuple[str, ...], start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> bool
reveal_type(str.startswith)
static_assert(is_assignable_to(TypeOf[str.startswith], Callable))

# revealed: <method-wrapper 'startswith' of string 'foo'>
reveal_type("foo".startswith)
static_assert(is_assignable_to(TypeOf["foo".startswith], Callable))

def _(
    a: RegularCallableTypeOf[types.FunctionType.__get__],
    b: RegularCallableTypeOf[f],
    c: RegularCallableTypeOf[f.__get__],
    d: RegularCallableTypeOf[types.FunctionType.__call__],
    e: RegularCallableTypeOf[f.__call__],
    f: RegularCallableTypeOf[property],
    g: RegularCallableTypeOf[property.__get__],
    h: RegularCallableTypeOf[MyClass.my_property.__get__],
    i: RegularCallableTypeOf[property.__set__],
    j: RegularCallableTypeOf[MyClass.my_property.__set__],
    k: RegularCallableTypeOf[str.startswith],
    l: RegularCallableTypeOf["foo".startswith],
):
    # revealed: Overload[(self: FunctionType, instance: None, owner: type, /) -> Unknown, (self: FunctionType, instance: object, owner: type | None = None, /) -> Unknown]
    reveal_type(a)

    # revealed: (obj: type) -> None
    reveal_type(b)

    # TODO: ideally this would have precise return types rather than `Unknown`
    # revealed: Overload[(instance: None, owner: type, /) -> Unknown, (instance: object, owner: type | None = None, /) -> Unknown]
    reveal_type(c)

    # revealed: (self, *args: Any, **kwargs: Any) -> Any
    reveal_type(d)

    # revealed: (obj: type) -> None
    reveal_type(e)

    # revealed: (fget: ((Any, /) -> Any) | None = None, fset: ((Any, Any, /) -> None) | None = None, fdel: ((Any, /) -> None) | None = None, doc: str | None = None) -> property
    reveal_type(f)

    # revealed: Overload[(self: property, instance: None, owner: type, /) -> Unknown, (self: property, instance: object, owner: type | None = None, /) -> Unknown]
    reveal_type(g)

    # TODO: ideally this would have precise return types rather than `Unknown`
    # revealed: Overload[(instance: None, owner: type, /) -> Unknown, (instance: object, owner: type | None = None, /) -> Unknown]
    reveal_type(h)

    # TODO: ideally this would have `-> None` rather than `-> Unknown`
    # revealed: (self: property, instance: object, value: object, /) -> Unknown
    reveal_type(i)

    # TODO: ideally this would have a more precise input type and `-> None` rather than `-> Unknown`
    # revealed: (instance: object, value: object, /) -> Unknown
    reveal_type(j)

    # revealed: (self, prefix: str | tuple[str, ...], start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> bool
    reveal_type(k)

    # revealed: (prefix: str | tuple[str, ...], start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> bool
    reveal_type(l)
```

[functions and methods]: https://docs.python.org/3/howto/descriptor.html#functions-and-methods
[`__init_subclass__`]: https://docs.python.org/3/reference/datamodel.html#object.__init_subclass__
