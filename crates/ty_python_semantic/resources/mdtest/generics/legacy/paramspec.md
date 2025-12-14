# Legacy `ParamSpec`

## Definition

### Valid

```py
from typing import ParamSpec

P = ParamSpec("P")
reveal_type(type(P))  # revealed: <class 'ParamSpec'>
reveal_type(P)  # revealed: ParamSpec
reveal_type(P.__name__)  # revealed: Literal["P"]
```

The paramspec name can also be provided as a keyword argument:

```py
from typing import ParamSpec

P = ParamSpec(name="P")
reveal_type(P.__name__)  # revealed: Literal["P"]
```

### Must be directly assigned to a variable

```py
from typing import ParamSpec

P = ParamSpec("P")
# error: [invalid-paramspec]
P1: ParamSpec = ParamSpec("P1")

# error: [invalid-paramspec]
tuple_with_typevar = ("foo", ParamSpec("W"))
reveal_type(tuple_with_typevar[1])  # revealed: ParamSpec
```

```py
from typing_extensions import ParamSpec

T = ParamSpec("T")
# error: [invalid-paramspec]
P1: ParamSpec = ParamSpec("P1")

# error: [invalid-paramspec]
tuple_with_typevar = ("foo", ParamSpec("P2"))
reveal_type(tuple_with_typevar[1])  # revealed: ParamSpec
```

### `ParamSpec` parameter must match variable name

```py
from typing import ParamSpec

P1 = ParamSpec("P1")

# error: [invalid-paramspec]
P2 = ParamSpec("P3")
```

### Accepts only a single `name` argument

> The runtime should accept bounds and covariant and contravariant arguments in the declaration just
> as typing.TypeVar does, but for now we will defer the standardization of the semantics of those
> options to a later PEP.

```py
from typing import ParamSpec

# error: [invalid-paramspec]
P1 = ParamSpec("P1", bound=int)
# error: [invalid-paramspec]
P2 = ParamSpec("P2", int, str)
# error: [invalid-paramspec]
P3 = ParamSpec("P3", covariant=True)
# error: [invalid-paramspec]
P4 = ParamSpec("P4", contravariant=True)
```

### Defaults

```toml
[environment]
python-version = "3.13"
```

The default value for a `ParamSpec` can be either a list of types, `...`, or another `ParamSpec`.

```py
from typing import ParamSpec

P1 = ParamSpec("P1", default=[int, str])
P2 = ParamSpec("P2", default=...)
P3 = ParamSpec("P3", default=P2)
```

Other values are invalid.

```py
# error: [invalid-paramspec]
P4 = ParamSpec("P4", default=int)
```

### `default` parameter in `typing_extensions.ParamSpec`

```toml
[environment]
python-version = "3.12"
```

The `default` parameter to `ParamSpec` is available from `typing_extensions` in Python 3.12 and
earlier.

```py
from typing import ParamSpec
from typing_extensions import ParamSpec as ExtParamSpec

# This shouldn't emit a diagnostic
P1 = ExtParamSpec("P1", default=[int, str])

# But, this should
# error: [invalid-paramspec] "The `default` parameter of `typing.ParamSpec` was added in Python 3.13"
P2 = ParamSpec("P2", default=[int, str])
```

And, it allows the same set of values as `typing.ParamSpec`.

```py
P3 = ExtParamSpec("P3", default=...)
P4 = ExtParamSpec("P4", default=P3)

# error: [invalid-paramspec]
P5 = ExtParamSpec("P5", default=int)
```

### Forward references in stub files

Stubs natively support forward references, so patterns that would raise `NameError` at runtime are
allowed in stub files:

```pyi
from typing_extensions import ParamSpec

P = ParamSpec("P", default=[A, B])

class A: ...
class B: ...
```

## Validating `ParamSpec` usage

In type annotations, `ParamSpec` is only valid as the first element to `Callable`, the final element
to `Concatenate`, or as a type parameter to `Protocol` or `Generic`.

```py
from typing import ParamSpec, Callable, Concatenate, Protocol, Generic

P = ParamSpec("P")

class ValidProtocol(Protocol[P]):
    def method(self, c: Callable[P, int]) -> None: ...

class ValidGeneric(Generic[P]):
    def method(self, c: Callable[P, int]) -> None: ...

def valid(
    a1: Callable[P, int],
    a2: Callable[Concatenate[int, P], int],
) -> None: ...
def invalid(
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
from typing import Generic, Callable, ParamSpec

P = ParamSpec("P")

def foo1(c: Callable[P, int]) -> None:
    def nested1(*args: P.args, **kwargs: P.kwargs) -> None: ...
    def nested2(
        # error: [invalid-type-form] "`P.kwargs` is valid only in `**kwargs` annotation: Did you mean `P.args`?"
        *args: P.kwargs,
        # error: [invalid-type-form] "`P.args` is valid only in `*args` annotation: Did you mean `P.kwargs`?"
        **kwargs: P.args,
    ) -> None: ...

    # TODO: error
    def nested3(*args: P.args) -> None: ...

    # TODO: error
    def nested4(**kwargs: P.kwargs) -> None: ...

    # TODO: error
    def nested5(*args: P.args, x: int, **kwargs: P.kwargs) -> None: ...

# TODO: error
def bar1(*args: P.args, **kwargs: P.kwargs) -> None:
    pass

class Foo1:
    # TODO: error
    def method(self, *args: P.args, **kwargs: P.kwargs) -> None: ...
```

And, they need to be used together.

```py
def foo2(c: Callable[P, int]) -> None:
    # TODO: error
    def nested1(*args: P.args) -> None: ...

    # TODO: error
    def nested2(**kwargs: P.kwargs) -> None: ...

class Foo2:
    # TODO: error
    args: P.args

    # TODO: error
    kwargs: P.kwargs
```

The name of these parameters does not need to be `args` or `kwargs`, it's the annotated type to the
respective variadic parameter that matters.

```py
class Foo3(Generic[P]):
    def method1(self, *paramspec_args: P.args, **paramspec_kwargs: P.kwargs) -> None: ...
    def method2(
        self,
        # error: [invalid-type-form] "`P.kwargs` is valid only in `**kwargs` annotation: Did you mean `P.args`?"
        *paramspec_args: P.kwargs,
        # error: [invalid-type-form] "`P.args` is valid only in `*args` annotation: Did you mean `P.kwargs`?"
        **paramspec_kwargs: P.args,
    ) -> None: ...
```

## Specializing generic classes explicitly

```py
from typing import Any, Generic, ParamSpec, Callable, TypeVar

P1 = ParamSpec("P1")
P2 = ParamSpec("P2")
T1 = TypeVar("T1")

class OnlyParamSpec(Generic[P1]):
    attr: Callable[P1, None]

class TwoParamSpec(Generic[P1, P2]):
    attr1: Callable[P1, None]
    attr2: Callable[P2, None]

class TypeVarAndParamSpec(Generic[T1, P1]):
    attr: Callable[P1, T1]
```

Explicit specialization of a generic class involving `ParamSpec` is done by providing either a list
of types, `...`, or another in-scope `ParamSpec`.

```py
reveal_type(OnlyParamSpec[[]]().attr)  # revealed: () -> None
reveal_type(OnlyParamSpec[[int, str]]().attr)  # revealed: (int, str, /) -> None
reveal_type(OnlyParamSpec[...]().attr)  # revealed: (...) -> None

def func(c: Callable[P2, None]):
    reveal_type(OnlyParamSpec[P2]().attr)  # revealed: (**P2@func) -> None

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
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be either a list of types, `ParamSpec`, `Concatenate`, or `...`"
reveal_type(TypeVarAndParamSpec[int, int]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be"
reveal_type(TypeVarAndParamSpec[int, ()]().attr)  # revealed: (...) -> int
# error: [invalid-type-arguments] "Type argument for `ParamSpec` must be"
reveal_type(TypeVarAndParamSpec[int, (int, str)]().attr)  # revealed: (...) -> int
```

Nor can they be omitted when there are more than one `ParamSpec`s.

```py
p = TwoParamSpec[[int, str], [int]]()
reveal_type(p.attr1)  # revealed: (int, str, /) -> None
reveal_type(p.attr2)  # revealed: (int, /) -> None

# error: [invalid-type-arguments]
# error: [invalid-type-arguments]
TwoParamSpec[int, str]
```

Specializing `ParamSpec` type variable using `typing.Any` isn't explicitly allowed by the spec but
both mypy and Pyright allow this and there are usages of this in the wild e.g.,
`staticmethod[Any, Any]`.

```py
reveal_type(TypeVarAndParamSpec[int, Any]().attr)  # revealed: (...) -> int
```

## Specialization when defaults are involved

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Any, Generic, ParamSpec, Callable, TypeVar

P = ParamSpec("P")
PList = ParamSpec("PList", default=[int, str])
PEllipsis = ParamSpec("PEllipsis", default=...)
PAnother = ParamSpec("PAnother", default=P)
PAnotherWithDefault = ParamSpec("PAnotherWithDefault", default=PList)
```

```py
class ParamSpecWithDefault1(Generic[PList]):
    attr: Callable[PList, None]

reveal_type(ParamSpecWithDefault1().attr)  # revealed: (int, str, /) -> None
reveal_type(ParamSpecWithDefault1[[int]]().attr)  # revealed: (int, /) -> None
```

```py
class ParamSpecWithDefault2(Generic[PEllipsis]):
    attr: Callable[PEllipsis, None]

reveal_type(ParamSpecWithDefault2().attr)  # revealed: (...) -> None
reveal_type(ParamSpecWithDefault2[[int, str]]().attr)  # revealed: (int, str, /) -> None
```

```py
class ParamSpecWithDefault3(Generic[P, PAnother]):
    attr1: Callable[P, None]
    attr2: Callable[PAnother, None]

# `P` hasn't been specialized, so it defaults to `Unknown` gradual form
p1 = ParamSpecWithDefault3()
reveal_type(p1.attr1)  # revealed: (...) -> None
reveal_type(p1.attr2)  # revealed: (...) -> None

p2 = ParamSpecWithDefault3[[int, str]]()
reveal_type(p2.attr1)  # revealed: (int, str, /) -> None
reveal_type(p2.attr2)  # revealed: (int, str, /) -> None

p3 = ParamSpecWithDefault3[[int], [str]]()
reveal_type(p3.attr1)  # revealed: (int, /) -> None
reveal_type(p3.attr2)  # revealed: (str, /) -> None

class ParamSpecWithDefault4(Generic[PList, PAnotherWithDefault]):
    attr1: Callable[PList, None]
    attr2: Callable[PAnotherWithDefault, None]

p1 = ParamSpecWithDefault4()
reveal_type(p1.attr1)  # revealed: (int, str, /) -> None
reveal_type(p1.attr2)  # revealed: (int, str, /) -> None

p2 = ParamSpecWithDefault4[[int]]()
reveal_type(p2.attr1)  # revealed: (int, /) -> None
reveal_type(p2.attr2)  # revealed: (int, /) -> None

p3 = ParamSpecWithDefault4[[int], [str]]()
reveal_type(p3.attr1)  # revealed: (int, /) -> None
reveal_type(p3.attr2)  # revealed: (str, /) -> None

# Un-ordered type variables as the default of `PAnother` is `P`
class ParamSpecWithDefault5(Generic[PAnother, P]):  # error: [invalid-generic-class]
    attr: Callable[PAnother, None]

# TODO: error
# PAnother has default as P (another ParamSpec) which is not in scope
class ParamSpecWithDefault6(Generic[PAnother]):
    attr: Callable[PAnother, None]
```

## Semantics

The semantics of `ParamSpec` are described in
[the PEP 695 `ParamSpec` document](./../pep695/paramspec.md) to avoid duplication unless there are
any behavior specific to the legacy `ParamSpec` implementation.
