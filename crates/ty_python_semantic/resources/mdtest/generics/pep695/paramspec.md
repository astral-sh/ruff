# PEP 695 `ParamSpec`

`ParamSpec` was introduced in Python 3.12 while the support for specifying defaults was added in
Python 3.13.

```toml
[environment]
python-version = "3.13"
```

## Definition

```py
def foo1[**P]() -> None:
    reveal_type(P)  # revealed: ParamSpec
```

## Bounds and constraints

`ParamSpec`, when defined using the new syntax, does not allow defining bounds or constraints.

TODO: This results in a lot of syntax errors mainly because the AST doesn't accept them in this
position. The parser could do a better job in recovering from these errors.

<!-- blacken-docs:off -->

```py
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
def foo[**P: int]() -> None:
    # error: [invalid-syntax]
    # error: [invalid-syntax]
    pass
```

<!-- blacken-docs:on -->

## Default

The default value for a `ParamSpec` can be either a list of types, `...`, or another `ParamSpec`.

```py
def foo2[**P = ...]() -> None:
    reveal_type(P)  # revealed: ParamSpec

def foo3[**P = [int, str]]() -> None:
    reveal_type(P)  # revealed: ParamSpec

def foo4[**P, **Q = P]():
    reveal_type(P)  # revealed: ParamSpec
    reveal_type(Q)  # revealed: ParamSpec
```

Other values are invalid.

```py
# error: [invalid-paramspec]
def foo[**P = int]() -> None:
    pass
```

## Validating `ParamSpec` usage

`ParamSpec` is only valid as the first element to `Callable` or the final element to `Concatenate`.

```py
from typing import ParamSpec, Callable, Concatenate

def valid[**P](
    a1: Callable[P, int],
    a2: Callable[Concatenate[int, P], int],
) -> None: ...
def invalid[**P](
    # TODO: error
    a1: P,
    # TODO: error
    a2: list[P],
    # TODO: error
    a3: Callable[[P], int],
    # TODO: error
    a4: Callable[..., P],
    # TODO: error
    a5: Callable[Concatenate[P, ...], int],
) -> None: ...
```

## Validating `P.args` and `P.kwargs` usage

The components of `ParamSpec` i.e., `P.args` and `P.kwargs` are only valid when used as the
annotated types of `*args` and `**kwargs` respectively.

```py
from typing import Callable

def foo[**P](c: Callable[P, int]) -> None:
    def nested1(*args: P.args, **kwargs: P.kwargs) -> None: ...

    # error: [invalid-type-form] "`P.kwargs` is valid only in `**kwargs` annotation: Did you mean `P.args`?"
    # error: [invalid-type-form] "`P.args` is valid only in `*args` annotation: Did you mean `P.kwargs`?"
    def nested2(*args: P.kwargs, **kwargs: P.args) -> None: ...

    # TODO: error
    def nested3(*args: P.args) -> None: ...

    # TODO: error
    def nested4(**kwargs: P.kwargs) -> None: ...

    # TODO: error
    def nested5(*args: P.args, x: int, **kwargs: P.kwargs) -> None: ...
```

And, they need to be used together.

```py
def foo[**P](c: Callable[P, int]) -> None:
    # TODO: error
    def nested1(*args: P.args) -> None: ...

    # TODO: error
    def nested2(**kwargs: P.kwargs) -> None: ...

class Foo[**P]:
    # TODO: error
    args: P.args

    # TODO: error
    kwargs: P.kwargs
```

The name of these parameters does not need to be `args` or `kwargs`, it's the annotated type to the
respective variadic parameter that matters.

```py
class Foo3[**P]:
    def method1(self, *paramspec_args: P.args, **paramspec_kwargs: P.kwargs) -> None: ...
    def method2(
        self,
        # error: [invalid-type-form] "`P.kwargs` is valid only in `**kwargs` annotation: Did you mean `P.args`?"
        *paramspec_args: P.kwargs,
        # error: [invalid-type-form] "`P.args` is valid only in `*args` annotation: Did you mean `P.kwargs`?"
        **paramspec_kwargs: P.args,
    ) -> None: ...
```

It isn't allowed to annotate an instance attribute either:

```py
class Foo4[**P]:
    def __init__(self, fn: Callable[P, int], *args: P.args, **kwargs: P.kwargs) -> None:
        self.fn = fn
        # TODO: error
        self.args: P.args = args
        # TODO: error
        self.kwargs: P.kwargs = kwargs
```

## Semantics of `P.args` and `P.kwargs`

The type of `args` and `kwargs` inside the function is `P.args` and `P.kwargs` respectively instead
of `tuple[P.args, ...]` and `dict[str, P.kwargs]`.

### Passing `*args` and `**kwargs` to a callable

```py
from typing import Callable

def f[**P](func: Callable[P, int]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        reveal_type(args)  # revealed: P@f.args
        reveal_type(kwargs)  # revealed: P@f.kwargs
        reveal_type(func(*args, **kwargs))  # revealed: int

        # error: [invalid-argument-type] "Argument is incorrect: Expected `P@f.args`, found `P@f.kwargs`"
        # error: [invalid-argument-type] "Argument is incorrect: Expected `P@f.kwargs`, found `P@f.args`"
        reveal_type(func(*kwargs, **args))  # revealed: int

        # error: [invalid-argument-type] "Argument is incorrect: Expected `P@f.args`, found `P@f.kwargs`"
        reveal_type(func(args, kwargs))  # revealed: int

        # Both parameters are required
        # TODO: error
        reveal_type(func())  # revealed: int
        reveal_type(func(*args))  # revealed: int
        reveal_type(func(**kwargs))  # revealed: int
    return wrapper
```

### Operations on `P.args` and `P.kwargs`

The type of `P.args` and `P.kwargs` behave like a `tuple` and `dict` respectively. Internally, they
are represented as a type variable that has an upper bound of `tuple[object, ...]` and
`Top[dict[str, Any]]` respectively.

```py
from typing import Callable, Any

def f[**P](func: Callable[P, int], *args: P.args, **kwargs: P.kwargs) -> None:
    reveal_type(args + ("extra",))  # revealed: tuple[object, ...]
    reveal_type(args + (1, 2, 3))  # revealed: tuple[object, ...]
    reveal_type(args[0])  # revealed: object

    reveal_type("key" in kwargs)  # revealed: bool
    reveal_type(kwargs.get("key"))  # revealed: object
    reveal_type(kwargs["key"])  # revealed: object
```

## Specializing generic classes explicitly

```py
from typing import Any, Callable, ParamSpec

class OnlyParamSpec[**P1]:
    attr: Callable[P1, None]

class TwoParamSpec[**P1, **P2]:
    attr1: Callable[P1, None]
    attr2: Callable[P2, None]

class TypeVarAndParamSpec[T1, **P1]:
    attr: Callable[P1, T1]
```

Explicit specialization of a generic class involving `ParamSpec` is done by providing either a list
of types, `...`, or another in-scope `ParamSpec`.

```py
reveal_type(OnlyParamSpec[[]]().attr)  # revealed: () -> None
reveal_type(OnlyParamSpec[[int, str]]().attr)  # revealed: (int, str, /) -> None
reveal_type(OnlyParamSpec[...]().attr)  # revealed: (...) -> None

def func[**P2](c: Callable[P2, None]):
    reveal_type(OnlyParamSpec[P2]().attr)  # revealed: (**P2@func) -> None

P2 = ParamSpec("P2")

# error: [invalid-type-arguments] "ParamSpec `P2` is unbound"
reveal_type(OnlyParamSpec[P2]().attr)  # revealed: (...) -> None

# error: [invalid-type-arguments] "No type argument provided for required type variable `P1` of class `OnlyParamSpec`"
reveal_type(OnlyParamSpec[()]().attr)  # revealed: (...) -> None
```

An explicit tuple expression (unlike an implicit one that omits the parentheses) is also accepted
when the `ParamSpec` is the only type variable. But, this isn't recommended is mainly a fallout of
it having the same AST as the one without the parentheses. Both mypy and Pyright also allow this.

```py
reveal_type(OnlyParamSpec[(int, str)]().attr)  # revealed: (int, str, /) -> None
```

<!-- blacken-docs:off -->

```py
# error: [invalid-syntax]
reveal_type(OnlyParamSpec[]().attr)  # revealed: (...) -> None
```

<!-- blacken-docs:on -->

The square brackets can be omitted when `ParamSpec` is the only type variable

```py
reveal_type(OnlyParamSpec[int, str]().attr)  # revealed: (int, str, /) -> None
reveal_type(OnlyParamSpec[int,]().attr)  # revealed: (int, /) -> None

# Even when there is only one element
reveal_type(OnlyParamSpec[Any]().attr)  # revealed: (Any, /) -> None
reveal_type(OnlyParamSpec[object]().attr)  # revealed: (object, /) -> None
reveal_type(OnlyParamSpec[int]().attr)  # revealed: (int, /) -> None
```

But, they cannot be omitted when there are multiple type variables.

```py
reveal_type(TypeVarAndParamSpec[int, []]().attr)  # revealed: () -> int
reveal_type(TypeVarAndParamSpec[int, [int, str]]().attr)  # revealed: (int, str, /) -> int
reveal_type(TypeVarAndParamSpec[int, [str]]().attr)  # revealed: (str, /) -> int
reveal_type(TypeVarAndParamSpec[int, ...]().attr)  # revealed: (...) -> int

# error: [invalid-type-arguments] "ParamSpec `P2` is unbound"
reveal_type(TypeVarAndParamSpec[int, P2]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be"
reveal_type(TypeVarAndParamSpec[int, int]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be"
reveal_type(TypeVarAndParamSpec[int, ()]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be"
reveal_type(TypeVarAndParamSpec[int, (int, str)]().attr)  # revealed: (...) -> int
```

Nor can they be omitted when there are more than one `ParamSpec`.

```py
p = TwoParamSpec[[int, str], [int]]()
reveal_type(p.attr1)  # revealed: (int, str, /) -> None
reveal_type(p.attr2)  # revealed: (int, /) -> None

# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be either a list of types, `ParamSpec`, `Concatenate`, or `...`"
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be either a list of types, `ParamSpec`, `Concatenate`, or `...`"
TwoParamSpec[int, str]
```

Specializing `ParamSpec` type variable using `typing.Any` isn't explicitly allowed by the spec but
both mypy and Pyright allow this and there are usages of this in the wild e.g.,
`staticmethod[Any, Any]`.

```py
reveal_type(TypeVarAndParamSpec[int, Any]().attr)  # revealed: (...) -> int
```

## Specialization when defaults are involved

```py
from typing import Callable, ParamSpec

class ParamSpecWithDefault1[**P1 = [int, str]]:
    attr: Callable[P1, None]

reveal_type(ParamSpecWithDefault1().attr)  # revealed: (int, str, /) -> None
reveal_type(ParamSpecWithDefault1[int]().attr)  # revealed: (int, /) -> None
```

```py
class ParamSpecWithDefault2[**P1 = ...]:
    attr: Callable[P1, None]

reveal_type(ParamSpecWithDefault2().attr)  # revealed: (...) -> None
reveal_type(ParamSpecWithDefault2[int, str]().attr)  # revealed: (int, str, /) -> None
```

```py
class ParamSpecWithDefault3[**P1, **P2 = P1]:
    attr1: Callable[P1, None]
    attr2: Callable[P2, None]

# `P1` hasn't been specialized, so it defaults to `...` gradual form
p1 = ParamSpecWithDefault3()
reveal_type(p1.attr1)  # revealed: (...) -> None
reveal_type(p1.attr2)  # revealed: (...) -> None

p2 = ParamSpecWithDefault3[[int, str]]()
reveal_type(p2.attr1)  # revealed: (int, str, /) -> None
reveal_type(p2.attr2)  # revealed: (int, str, /) -> None

p3 = ParamSpecWithDefault3[[int], [str]]()
reveal_type(p3.attr1)  # revealed: (int, /) -> None
reveal_type(p3.attr2)  # revealed: (str, /) -> None

class ParamSpecWithDefault4[**P1 = [int, str], **P2 = P1]:
    attr1: Callable[P1, None]
    attr2: Callable[P2, None]

p1 = ParamSpecWithDefault4()
reveal_type(p1.attr1)  # revealed: (int, str, /) -> None
reveal_type(p1.attr2)  # revealed: (int, str, /) -> None

p2 = ParamSpecWithDefault4[[int]]()
reveal_type(p2.attr1)  # revealed: (int, /) -> None
reveal_type(p2.attr2)  # revealed: (int, /) -> None

p3 = ParamSpecWithDefault4[[int], [str]]()
reveal_type(p3.attr1)  # revealed: (int, /) -> None
reveal_type(p3.attr2)  # revealed: (str, /) -> None

P2 = ParamSpec("P2")

# TODO: error: paramspec is out of scope
class ParamSpecWithDefault5[**P1 = P2]:
    attr: Callable[P1, None]
```

## Semantics

Most of these test cases are adopted from the
[typing documentation on `ParamSpec` semantics](https://typing.python.org/en/latest/spec/generics.html#semantics).

### Return type change using `ParamSpec` once

```py
from typing import Callable

def converter[**P](func: Callable[P, int]) -> Callable[P, bool]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> bool:
        func(*args, **kwargs)
        return True
    return wrapper

def f1(x: int, y: str) -> int:
    return 1

# This should preserve all the information about the parameters of `f1`
f2 = converter(f1)

reveal_type(f2)  # revealed: (x: int, y: str) -> bool

reveal_type(f1(1, "a"))  # revealed: int
reveal_type(f2(1, "a"))  # revealed: bool

# As it preserves the parameter kinds, the following should work as well
reveal_type(f2(1, y="a"))  # revealed: bool
reveal_type(f2(x=1, y="a"))  # revealed: bool
reveal_type(f2(y="a", x=1))  # revealed: bool

# error: [missing-argument] "No argument provided for required parameter `y`"
f2(1)
# error: [invalid-argument-type] "Argument is incorrect: Expected `int`, found `Literal["a"]`"
f2("a", "b")
```

The `converter` function act as a decorator here:

```py
@converter
def f3(x: int, y: str) -> int:
    return 1

reveal_type(f3)  # revealed: (x: int, y: str) -> bool

reveal_type(f3(1, "a"))  # revealed: bool
reveal_type(f3(x=1, y="a"))  # revealed: bool
reveal_type(f3(1, y="a"))  # revealed: bool
reveal_type(f3(y="a", x=1))  # revealed: bool

# error: [missing-argument] "No argument provided for required parameter `y`"
f3(1)
# error: [invalid-argument-type] "Argument is incorrect: Expected `int`, found `Literal["a"]`"
f3("a", "b")
```

### Return type change using the same `ParamSpec` multiple times

```py
from typing import Callable

def multiple[**P](func1: Callable[P, int], func2: Callable[P, int]) -> Callable[P, bool]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> bool:
        func1(*args, **kwargs)
        func2(*args, **kwargs)
        return True
    return wrapper
```

As per the spec,

> A user may include the same `ParamSpec` multiple times in the arguments of the same function, to
> indicate a dependency between multiple arguments. In these cases a type checker may choose to
> solve to a common behavioral supertype (i.e. a set of parameters for which all of the valid calls
> are valid in both of the subtypes), but is not obligated to do so.

TODO: Currently, we don't do this

```py
def xy(x: int, y: str) -> int:
    return 1

def yx(y: int, x: str) -> int:
    return 2

reveal_type(multiple(xy, xy))  # revealed: (x: int, y: str) -> bool

# The common supertype is `(int, str, /)` which is converting the positional-or-keyword parameters
# into positional-only parameters because the position of the types are the same.
# TODO: This shouldn't error
# error: [invalid-argument-type]
reveal_type(multiple(xy, yx))  # revealed: (x: int, y: str) -> bool

def keyword_only_with_default_1(*, x: int = 42) -> int:
    return 1

def keyword_only_with_default_2(*, y: int = 42) -> int:
    return 2

# The common supertype for two functions with only keyword-only parameters would be an empty
# parameter list i.e., `()`
# TODO: This shouldn't error
# error: [invalid-argument-type]
# revealed: (*, x: int = 42) -> bool
reveal_type(multiple(keyword_only_with_default_1, keyword_only_with_default_2))

def keyword_only1(*, x: int) -> int:
    return 1

def keyword_only2(*, y: int) -> int:
    return 2

# On the other hand, combining two functions with only keyword-only parameters does not have a
# common supertype, so it should result in an error.
# error: [invalid-argument-type] "Argument to function `multiple` is incorrect: Expected `(*, x: int) -> int`, found `def keyword_only2(*, y: int) -> int`"
reveal_type(multiple(keyword_only1, keyword_only2))  # revealed: (*, x: int) -> bool
```

### Constructors of user-defined generic class on `ParamSpec`

```py
from typing import Callable

class C[**P]:
    f: Callable[P, int]

    def __init__(self, f: Callable[P, int]) -> None:
        self.f = f

# Note that the return type must match exactly, since C is invariant on the return type of C.f.
def f(x: int, y: str) -> int:
    return True

c = C(f)
reveal_type(c.f)  # revealed: (x: int, y: str) -> int
```

### `ParamSpec` in prepended positional parameters

> If one of these prepended positional parameters contains a free `ParamSpec`, we consider that
> variable in scope for the purposes of extracting the components of that `ParamSpec`.

```py
from typing import Callable

def foo1[**P1](func: Callable[P1, int], *args: P1.args, **kwargs: P1.kwargs) -> int:
    return func(*args, **kwargs)

def foo1_with_extra_arg[**P1](func: Callable[P1, int], extra: str, *args: P1.args, **kwargs: P1.kwargs) -> int:
    return func(*args, **kwargs)

def foo2[**P2](func: Callable[P2, int], *args: P2.args, **kwargs: P2.kwargs) -> None:
    foo1(func, *args, **kwargs)

    # error: [invalid-argument-type] "Argument to function `foo1` is incorrect: Expected `P2@foo2.args`, found `Literal[1]`"
    foo1(func, 1, *args, **kwargs)

    # error: [invalid-argument-type] "Argument to function `foo1_with_extra_arg` is incorrect: Expected `str`, found `P2@foo2.args`"
    foo1_with_extra_arg(func, *args, **kwargs)

    foo1_with_extra_arg(func, "extra", *args, **kwargs)
```

Here, the first argument to `f` can specialize `P` to the parameters of the callable passed to it
which is then used to type the `ParamSpec` components used in `*args` and `**kwargs`.

```py
def f1(x: int, y: str) -> int:
    return 1

foo1(f1, 1, "a")
foo1(f1, x=1, y="a")
foo1(f1, 1, y="a")

# error: [missing-argument] "No arguments provided for required parameters `x`, `y` of function `foo1`"
foo1(f1)

# error: [missing-argument] "No argument provided for required parameter `y` of function `foo1`"
foo1(f1, 1)

# error: [invalid-argument-type] "Argument to function `foo1` is incorrect: Expected `str`, found `Literal[2]`"
foo1(f1, 1, 2)

# error: [too-many-positional-arguments] "Too many positional arguments to function `foo1`: expected 2, got 3"
foo1(f1, 1, "a", "b")

# error: [missing-argument] "No argument provided for required parameter `y` of function `foo1`"
# error: [unknown-argument] "Argument `z` does not match any known parameter of function `foo1`"
foo1(f1, x=1, z="a")
```

### Specializing `ParamSpec` with another `ParamSpec`

```py
class Foo[**P]:
    def __init__(self, *args: P.args, **kwargs: P.kwargs) -> None:
        self.args = args
        self.kwargs = kwargs

def bar[**P](foo: Foo[P]) -> None:
    reveal_type(foo)  # revealed: Foo[P@bar]
    reveal_type(foo.args)  # revealed: Unknown | P@bar.args
    reveal_type(foo.kwargs)  # revealed: Unknown | P@bar.kwargs
```

ty will check whether the argument after `**` is a mapping type but as instance attribute are
unioned with `Unknown`, it shouldn't error here.

```py
from typing import Callable

def baz[**P](fn: Callable[P, None], foo: Foo[P]) -> None:
    fn(*foo.args, **foo.kwargs)
```

The `Unknown` can be eliminated by using annotating these attributes with `Final`:

```py
from typing import Final

class FooWithFinal[**P]:
    def __init__(self, *args: P.args, **kwargs: P.kwargs) -> None:
        self.args: Final = args
        self.kwargs: Final = kwargs

def with_final[**P](foo: FooWithFinal[P]) -> None:
    reveal_type(foo)  # revealed: FooWithFinal[P@with_final]
    reveal_type(foo.args)  # revealed: P@with_final.args
    reveal_type(foo.kwargs)  # revealed: P@with_final.kwargs
```

### Specializing `Self` when `ParamSpec` is involved

```py
class Foo[**P]:
    def method(self, *args: P.args, **kwargs: P.kwargs) -> str:
        return "hello"

foo = Foo[int, str]()

reveal_type(foo)  # revealed: Foo[(int, str, /)]
reveal_type(foo.method)  # revealed: bound method Foo[(int, str, /)].method(int, str, /) -> str
reveal_type(foo.method(1, "a"))  # revealed: str
```

### Gradual types propagate through `ParamSpec` inference

```py
from typing import Callable

def callable_identity[**P, R](func: Callable[P, R]) -> Callable[P, R]:
    return func

@callable_identity
def f(env: dict) -> None:
    pass

# revealed: (env: dict[Unknown, Unknown]) -> None
reveal_type(f)
```

### Overloads

`overloaded.pyi`:

```pyi
from typing import overload

@overload
def int_int(x: int) -> int: ...
@overload
def int_int(x: str) -> int: ...

@overload
def int_str(x: int) -> int: ...
@overload
def int_str(x: str) -> str: ...

@overload
def str_str(x: int) -> str: ...
@overload
def str_str(x: str) -> str: ...
```

```py
from typing import Callable
from overloaded import int_int, int_str, str_str

def change_return_type[**P](f: Callable[P, int]) -> Callable[P, str]:
    def nested(*args: P.args, **kwargs: P.kwargs) -> str:
        return str(f(*args, **kwargs))
    return nested

def with_parameters[**P](f: Callable[P, int], *args: P.args, **kwargs: P.kwargs) -> Callable[P, str]:
    def nested(*args: P.args, **kwargs: P.kwargs) -> str:
        return str(f(*args, **kwargs))
    return nested

reveal_type(change_return_type(int_int))  # revealed: Overload[(x: int) -> str, (x: str) -> str]

# TODO: This shouldn't error and should pick the first overload because of the return type
# error: [invalid-argument-type]
reveal_type(change_return_type(int_str))  # revealed: Overload[(x: int) -> str, (x: str) -> str]

# error: [invalid-argument-type]
reveal_type(change_return_type(str_str))  # revealed: (...) -> str

# TODO: Both of these shouldn't raise an error
# error: [invalid-argument-type]
reveal_type(with_parameters(int_int, 1))  # revealed: Overload[(x: int) -> str, (x: str) -> str]
# error: [invalid-argument-type]
reveal_type(with_parameters(int_int, "a"))  # revealed: Overload[(x: int) -> str, (x: str) -> str]
```

## ParamSpec attribute assignability

When comparing signatures with `ParamSpec` attributes (`P.args` and `P.kwargs`), two different
inferable `ParamSpec` attributes with the same kind are assignable to each other. This enables
method overrides where both methods have their own `ParamSpec`.

### Same attribute kind, both inferable

```py
from typing import Callable

class Parent:
    def method[**P](self, callback: Callable[P, None]) -> Callable[P, None]:
        return callback

class Child1(Parent):
    # This is a valid override: Q.args matches P.args, Q.kwargs matches P.kwargs
    def method[**Q](self, callback: Callable[Q, None]) -> Callable[Q, None]:
        return callback

# Both signatures use ParamSpec, so they should be compatible
def outer[**P](f: Callable[P, int]) -> Callable[P, int]:
    def inner[**Q](g: Callable[Q, int]) -> Callable[Q, int]:
        return g
    return inner(f)
```

We can explicitly mark it as an override using the `@override` decorator.

```py
from typing import override

class Child2(Parent):
    @override
    def method[**Q](self, callback: Callable[Q, None]) -> Callable[Q, None]:
        return callback
```

### One `ParamSpec` not inferable

Here, `P` is in a non-inferable position while `Q` is inferable. So, they are not considered
assignable.

```py
from typing import Callable

class Container[**P]:
    def method(self, f: Callable[P, None]) -> Callable[P, None]:
        return f

    def try_assign[**Q](self, f: Callable[Q, None]) -> Callable[Q, None]:
        # error: [invalid-return-type] "Return type does not match returned value: expected `(**Q@try_assign) -> None`, found `(**P@Container) -> None`"
        # error: [invalid-argument-type] "Argument to bound method `method` is incorrect: Expected `(**P@Container) -> None`, found `(**Q@try_assign) -> None`"
        return self.method(f)
```
