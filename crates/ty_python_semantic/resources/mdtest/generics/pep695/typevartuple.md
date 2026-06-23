# PEP 695 `TypeVarTuple`

```toml
[environment]
python-version = "3.12"
```

## Definition

A PEP 695 type variable tuple is introduced with a single starred type parameter.

```py
def foo[*Ts](*args: *Ts) -> None:
    reveal_type(Ts)  # revealed: TypeVarTuple
    reveal_type(args)  # revealed: tuple[*Ts@foo]
```

## Variance inference

PEP 695 type variable tuples infer variance from how the class uses them.

```py
class CovariantArray[*Ts]:
    def get(self) -> tuple[*Ts]:
        raise NotImplementedError

covariant_ok: CovariantArray[object] = CovariantArray[int]()
covariant_error: CovariantArray[int] = CovariantArray[object]()  # error: [invalid-assignment]

class ContravariantArray[*Ts]:
    def set(self, value: tuple[*Ts]) -> None:
        raise NotImplementedError

contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # error: [invalid-assignment]

class InvariantArray[*Ts]:
    values: tuple[*Ts]

invariant_out: InvariantArray[object] = InvariantArray[int]()  # error: [invalid-assignment]
invariant_in: InvariantArray[int] = InvariantArray[object]()  # error: [invalid-assignment]
```

## Generic Classes

### Explicit specialization

```py
class Simple[*Ts]:
    attr: tuple[*Ts]

reveal_type(Simple[()]().attr)  # revealed: tuple[()]
reveal_type(Simple[int, str]().attr)  # revealed: tuple[int, str]
reveal_type(Simple[*tuple[int, str]]().attr)  # revealed: tuple[int, str]

# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
reveal_type(Simple[[int, str]]().attr)  # revealed: tuple[Unknown]
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
reveal_type(Simple[*[int, str]]().attr)  # revealed: tuple[Unknown, ...]
```

```py
class Prefix[T, *Ts]:
    attr: tuple[T, *Ts]

reveal_type(Prefix[int]().attr)  # revealed: tuple[int]
reveal_type(Prefix[int, bool]().attr)  # revealed: tuple[int, bool]
reveal_type(Prefix[int, bool, str]().attr)  # revealed: tuple[int, bool, str]
reveal_type(Prefix[int, *tuple[bool, str]]().attr)  # revealed: tuple[int, bool, str]

# TODO: Should this raise an error?
reveal_type(Prefix().attr)  # revealed: tuple[Unknown, *tuple[Unknown, ...]]
```

```py
class Suffix[*Ts, T]:
    attr: tuple[*Ts, T]

reveal_type(Suffix[int]().attr)  # revealed: tuple[int]
reveal_type(Suffix[int, str]().attr)  # revealed: tuple[int, str]
reveal_type(Suffix[int, str, bool]().attr)  # revealed: tuple[int, str, bool]
reveal_type(Suffix[*tuple[int, str], bool]().attr)  # revealed: tuple[int, str, bool]

# TODO: Should this raise an error?
reveal_type(Suffix().attr)  # revealed: tuple[*tuple[Unknown, ...], Unknown]
```

```py
class Between[T, *Ts, U]:
    attr: tuple[T, *Ts, U]

reveal_type(Between[int, str]().attr)  # revealed: tuple[int, str]
reveal_type(Between[int, bool, str]().attr)  # revealed: tuple[int, bool, str]
reveal_type(Between[int, bool, bytes, str]().attr)  # revealed: tuple[int, bool, bytes, str]
reveal_type(Between[int, *tuple[bool], str]().attr)  # revealed: tuple[int, bool, str]

reveal_type(Between().attr)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
# error: [invalid-type-arguments] "No type argument provided for required type variable `U` of class `Between`"
reveal_type(Between[int]().attr)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
```

### `TypeVarTuple` with `ParamSpec`

```py
from typing import Callable

class TypeVarTupleWithParamSpec[*Ts, **P]:
    fn: Callable[P, tuple[*Ts]]

reveal_type(TypeVarTupleWithParamSpec[[str, int]]().fn)  # revealed: (str, int, /) -> tuple[()]
reveal_type(TypeVarTupleWithParamSpec[int, [str, int]]().fn)  # revealed: (str, int, /) -> tuple[int]
reveal_type(TypeVarTupleWithParamSpec[int, str, [str, int]]().fn)  # revealed: (str, int, /) -> tuple[int, str]

# error: [invalid-type-arguments]
reveal_type(TypeVarTupleWithParamSpec[str, int]().fn)  # revealed: (...) -> tuple[str]

reveal_type(TypeVarTupleWithParamSpec[str, int, []]().fn)  # revealed: () -> tuple[str, int]
reveal_type(TypeVarTupleWithParamSpec[str, int, ...]().fn)  # revealed: (...) -> tuple[str, int]
```

### Inferred specialization from construction

Calling a generic class without explicit type arguments infers its specialization from the
constructor arguments.

```py
class Positional[*Ts]:
    def __init__(self, shape: tuple[*Ts]) -> None:
        self.shape = shape

class Variadic[*Ts]:
    def __init__(self, *shape: *Ts) -> None:
        self.shape = shape

reveal_type(Positional(()))  # revealed: Positional[()]
reveal_type(Positional((1, "a")))  # revealed: Positional[int, str]

reveal_type(Variadic())  # revealed: Variadic[()]
reveal_type(Variadic(1, "a"))  # revealed: Variadic[int, str]

def _(i: int, s: str) -> None:
    reveal_type(Positional((i, s)))  # revealed: Positional[int, str]
    reveal_type(Variadic(i, s))  # revealed: Variadic[int, str]
```

### Unspecified type arguments

An unsubscripted variadic generic behaves as if it used an unknown-length tuple of `Any` arguments.
ty represents the missing type information as `Unknown`, distinguishing it from explicitly provided
`Any`.

```py
class Unspecified[*Ts]:
    attr: tuple[*Ts]

unspecified = Unspecified()
reveal_type(unspecified)  # revealed: Unspecified[*tuple[Unknown, ...]]
reveal_type(unspecified.attr)  # revealed: tuple[Unknown, ...]
```

### Default type arguments

A defaulted type variable tuple supplies its unpacked tuple when the generic class is not explicitly
specialized. Explicit type arguments override the default.

```toml
[environment]
python-version = "3.13"
```

```py
class WithDefault[*Ts = *tuple[int, str]]:
    attr: tuple[*Ts]

reveal_type(WithDefault().attr)  # revealed: tuple[int, str]
reveal_type(WithDefault[bool, bytes]().attr)  # revealed: tuple[bool, bytes]
```

### Gradual specializations

A type variable tuple remains assignable to an explicitly gradual specialization of its generic
class.

```py
from typing import Any

class Array[*Ts]:
    def erase_shape(self) -> "Array[*tuple[Any, ...]]":
        return self
```

## Functions

### Tuple arguments and returns

```py
def simple[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def with_prefix[T, *Ts](x: T, y: tuple[*Ts]) -> tuple[T, *Ts]:
    raise NotImplementedError

def with_suffix[*Ts, U](x: tuple[*Ts], y: U) -> tuple[*Ts, U]:
    raise NotImplementedError

def both[T, *Ts, U](x: T, y: tuple[*Ts], z: U) -> tuple[T, *Ts, U]:
    raise NotImplementedError

def f(i: int, s: str, b: bool, t: tuple[int, str], vt: tuple[int, ...]) -> None:
    reveal_type(simple(()))  # revealed: tuple[()]
    reveal_type(simple((i, s)))  # revealed: tuple[int, str]
    reveal_type(simple(t))  # revealed: tuple[int, str]
    reveal_type(simple(vt))  # revealed: tuple[int, ...]

    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[int, str, bool]
    reveal_type(with_prefix(i, t))  # revealed: tuple[int, int, str]
    reveal_type(with_prefix(i, vt))  # revealed: tuple[int, *tuple[int, ...]]
    reveal_type(with_prefix(t, vt))  # revealed: tuple[tuple[int, str], *tuple[int, ...]]

    reveal_type(with_suffix((i, s), b))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix(t, b))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix(vt, b))  # revealed: tuple[*tuple[int, ...], bool]
    reveal_type(with_suffix(vt, t))  # revealed: tuple[*tuple[int, ...], tuple[int, str]]

    reveal_type(both(i, (i, s), b))  # revealed: tuple[int, int, str, bool]
    reveal_type(both(i, t, b))  # revealed: tuple[int, int, str, bool]
    reveal_type(both(i, vt, b))  # revealed: tuple[int, *tuple[int, ...], bool]

    # TODO: Avoid also reporting an invalid argument type for the first unpacked element.
    # error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `tuple[Unknown, ...]`, found `int`"
    # error: [too-many-positional-arguments] "Too many positional arguments to function `simple`: expected 1, got 2"
    reveal_type(simple(*t))  # revealed: tuple[Unknown, ...]
```

### Starred variadic parameters

When a `TypeVarTuple` appears in `*args`, the argument tuple keeps the exact element types from the
call site.

```py
def simple[*Ts](*args: *Ts) -> tuple[*Ts]:
    reveal_type(args)  # revealed: tuple[*Ts@simple]
    raise NotImplementedError

def with_prefix[T, *Ts](prefix: T, *args: *Ts) -> tuple[T, *Ts]:
    raise NotImplementedError

def with_kw_only[T, *Ts](*args: *Ts, kw: T) -> tuple[*Ts, T]:
    raise NotImplementedError

def f(i: int, s: str, b: bool, t: tuple[int, str], vt: tuple[int, ...]) -> None:
    reveal_type(simple())  # revealed: tuple[()]
    reveal_type(simple(i, s))  # revealed: tuple[int, str]
    # TODO: Should reveal `tuple[int, str]`.
    reveal_type(simple(*(i, s)))  # revealed: tuple[tuple[int, str], tuple[int, str]]
    reveal_type(simple(t))  # revealed: tuple[tuple[int, str]]
    # TODO: Should reveal `tuple[int, str]`.
    reveal_type(simple(*t))  # revealed: tuple[tuple[int, str], tuple[int, str]]
    # TODO: Should reveal `tuple[int, ...]`.
    reveal_type(simple(*vt))  # revealed: tuple[tuple[int, ...]]

    reveal_type(with_prefix(i))  # revealed: tuple[int]
    reveal_type(with_prefix(i, s, b))  # revealed: tuple[int, str, bool]
    # TODO: Should reveal `tuple[int, str]`.
    reveal_type(with_prefix(*t))  # revealed: tuple[int, tuple[int, str]]
    # TODO: Should reveal `tuple[int, int, str]`.
    reveal_type(with_prefix(i, *t))  # revealed: tuple[int, tuple[int, str], tuple[int, str]]
    # TODO: Should reveal `tuple[int, *tuple[int, ...]]`.
    reveal_type(with_prefix(*vt))  # revealed: tuple[int, tuple[int, ...]]
    # TODO: Should reveal `tuple[int, *tuple[int, ...]]`.
    reveal_type(with_prefix(i, *vt))  # revealed: tuple[int, tuple[int, ...]]

    reveal_type(with_kw_only(kw=b))  # revealed: tuple[bool]
    reveal_type(with_kw_only(i, s, kw=b))  # revealed: tuple[int, str, bool]
    reveal_type(with_kw_only(t, kw=b))  # revealed: tuple[tuple[int, str], bool]
    # TODO: Should reveal `tuple[int, str, bool]`.
    reveal_type(with_kw_only(*t, kw=b))  # revealed: tuple[tuple[int, str], tuple[int, str], bool]
    reveal_type(with_kw_only(vt, kw=b))  # revealed: tuple[tuple[int, ...], bool]
    # TODO: Should reveal `tuple[*tuple[int, ...], bool]`.
    reveal_type(with_kw_only(*vt, kw=b))  # revealed: tuple[tuple[int, ...], bool]

    # error: [missing-argument] "No argument provided for required parameter `kw` of function `with_kw_only`"
    reveal_type(with_kw_only(i, s, b))  # revealed: tuple[int, str, bool, Unknown]
```

### Callable inference

`Callable` accepts unpacked `TypeVarTuple`s in its positional parameter list.

```py
from typing import Callable

def simple[*Ts](callback: Callable[[*Ts], tuple[*Ts]]) -> tuple[*Ts]:
    reveal_type(callback)  # revealed: (*Ts@simple) -> tuple[*Ts@simple]
    raise NotImplementedError

def positional_only(x: int, y: str, /) -> tuple[int, str]:
    raise NotImplementedError

def no_parameters() -> tuple[()]:
    raise NotImplementedError

def standard(x: int, y: str) -> tuple[int, str]:
    raise NotImplementedError

def positional_variadic(x: int, *args: str) -> tuple[int, *tuple[str, ...]]:
    raise NotImplementedError

def variadic1(*args: int) -> tuple[int, ...]:
    raise NotImplementedError

def variadic2(*args: int) -> tuple[str, ...]:
    raise NotImplementedError

def accepts_object(value: object, /) -> tuple[int]:
    raise NotImplementedError

def keyword_only(*, x: int) -> tuple[int]:
    raise NotImplementedError

def gradual(callback: Callable[..., tuple[int, ...]]) -> None:
    reveal_type(simple(callback))  # revealed: tuple[int, ...]

reveal_type(simple(no_parameters))  # revealed: tuple[()]
reveal_type(simple(positional_only))  # revealed: tuple[int, str]
reveal_type(simple(standard))  # revealed: tuple[int, str]
reveal_type(simple(positional_variadic))  # revealed: tuple[int, *tuple[str, ...]]
reveal_type(simple(variadic1))  # revealed: tuple[int, ...]
reveal_type(simple(accepts_object))  # revealed: tuple[int]

# TODO: Report the incompatible return type after callable specialization fails.
reveal_type(simple(variadic2))  # revealed: tuple[Unknown, ...]
# error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `(*args: Unknown) -> tuple[Unknown, ...]`, found `def keyword_only(*, x: int) -> tuple[int]`"
reveal_type(simple(keyword_only))  # revealed: tuple[Unknown, ...]
```

### Callable return inference

An unpacked `TypeVarTuple` in a callable return type is inferred as one packed tuple, including
fixed elements surrounding it.

```py
from typing import Callable

def infer_return[*Ts](callback: Callable[[], tuple[*Ts]]) -> tuple[*Ts]:
    raise NotImplementedError

def empty_return() -> tuple[()]:
    raise NotImplementedError

def fixed_return() -> tuple[int, str]:
    raise NotImplementedError

def mixed_return() -> tuple[int, *tuple[str, ...]]:
    raise NotImplementedError

reveal_type(infer_return(empty_return))  # revealed: tuple[()]
reveal_type(infer_return(fixed_return))  # revealed: tuple[int, str]
reveal_type(infer_return(mixed_return))  # revealed: tuple[int, *tuple[str, ...]]

def infer_return_middle[*Ts](
    callback: Callable[[], tuple[int, *Ts, bytes]],
) -> tuple[*Ts]:
    raise NotImplementedError

def fixed_middle() -> tuple[int, str, bytes]:
    raise NotImplementedError

def mixed_middle() -> tuple[int, *tuple[str, ...], bytes]:
    raise NotImplementedError

reveal_type(infer_return_middle(fixed_middle))  # revealed: tuple[str]
reveal_type(infer_return_middle(mixed_middle))  # revealed: tuple[str, ...]
```

### Callable inference with sub-call checking

This usage pattern is similar to how `ParamSpec` can be used to accept a callable and its arguments
except that in the case of `TypeVarTuple` all parameters are positional-only.

```py
from typing import Callable

def invoke[*Ts, R](callback: Callable[[*Ts], R], *args: *Ts) -> R:
    raise NotImplementedError

def positional_only(x: int, y: str, /) -> tuple[int, str]:
    raise NotImplementedError

def standard(x: int, y: str) -> tuple[int, str]:
    raise NotImplementedError

def positional_variadic(x: int, *args: str) -> tuple[int, *tuple[str, ...]]:
    raise NotImplementedError

reveal_type(invoke(positional_only, 1, "a"))  # revealed: tuple[int, str]
# error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `() -> tuple[int, str]`, found `def positional_only(x: int, y: str, /) -> tuple[int, str]`"
reveal_type(invoke(positional_only))  # revealed: tuple[int, str]
# error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `(Literal[1], /) -> tuple[int, str]`, found `def positional_only(x: int, y: str, /) -> tuple[int, str]`"
reveal_type(invoke(positional_only, 1))  # revealed: tuple[int, str]
# error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `(int, Literal[2] | str, /) -> tuple[int, str]`, found `def positional_only(x: int, y: str, /) -> tuple[int, str]`"
reveal_type(invoke(positional_only, 1, 2))  # revealed: tuple[int, str]

reveal_type(invoke(standard, 1, "a"))  # revealed: tuple[int, str]
# error: [unknown-argument] "Argument `x` does not match any known parameter of function `invoke`"
# error: [unknown-argument] "Argument `y` does not match any known parameter of function `invoke`"
# error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `() -> tuple[int, str]`, found `def standard(x: int, y: str) -> tuple[int, str]`"
reveal_type(invoke(standard, x=1, y="a"))  # revealed: tuple[int, str]

reveal_type(invoke(positional_variadic, 1, "a", "b"))  # revealed: tuple[int, *tuple[str, ...]]
reveal_type(invoke(positional_variadic, 1))  # revealed: tuple[int, *tuple[str, ...]]
# error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `() -> tuple[int, *tuple[str, ...]]`, found `def positional_variadic(x: int, *args: str) -> tuple[int, *tuple[str, ...]]`"
reveal_type(invoke(positional_variadic))  # revealed: tuple[int, *tuple[str, ...]]
```

### Callable inference with fixed positional parameters

Fixed positional parameters surrounding an unpacked `TypeVarTuple` are excluded from the inferred
tuple.

```py
from typing import Callable

def infer_with_suffix[*Ts](callback: Callable[[int, *Ts, bytes], None]) -> tuple[*Ts]:
    raise NotImplementedError

def fixed_suffix(prefix: int, middle: str, suffix: bytes, /) -> None: ...
def empty_middle(prefix: int, suffix: bytes, /) -> None: ...
def unpacked_suffix(*args: *tuple[int, *tuple[str, ...], bytes]) -> None: ...

reveal_type(infer_with_suffix(fixed_suffix))  # revealed: tuple[str]
reveal_type(infer_with_suffix(empty_middle))  # revealed: tuple[()]
reveal_type(infer_with_suffix(unpacked_suffix))  # revealed: tuple[str, ...]
```

### Callable inference with additional keyword parameters

Additional keyword-only or variadic keyword parameters do not contribute to a `TypeVarTuple`
inferred from a `Callable`'s positional parameter list.

```py
from typing import Callable

def infer_positional[*Ts](callback: Callable[[*Ts], None]) -> tuple[*Ts]:
    raise NotImplementedError

def optional_keyword_only(x: int, y: str, *, debug: bool = False) -> None: ...
def extra_keywords(x: int, y: str, **kwargs: bool) -> None: ...

reveal_type(infer_positional(optional_keyword_only))  # revealed: tuple[int, str]
reveal_type(infer_positional(extra_keywords))  # revealed: tuple[int, str]
```

### Callable protocol inference

`Callable[[*Ts], R]` can only describe positional-only parameters. Callable protocols are used below
to test `TypeVarTuple` inference for signatures that combine variadic positional parameters with
keyword-only or variadic keyword parameters.

#### Keyword-only parameters

A callable protocol can combine a `TypeVarTuple` with required or optional keyword-only parameters
and a fixed positional prefix.

```py
from typing import Protocol

class KeywordOnlyCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, flag: bool) -> None: ...

def infer_keyword_only[*Ts](callback: KeywordOnlyCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def explicit_keyword_only(x: int, y: str, *, flag: bool) -> None: ...
def positional_only_with_keyword(x: int, y: str, /, *, flag: bool) -> None: ...
def positional_or_keyword(x: int, y: str, flag: bool) -> None: ...
def keyword_catch_all(x: int, y: str, **kwargs: object) -> None: ...

# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_keyword_only` is incorrect: Expected `KeywordOnlyCallback[*tuple[Unknown, ...]]`, found `def explicit_keyword_only(x: int, y: str, *, flag: bool) -> None`"
reveal_type(infer_keyword_only(explicit_keyword_only))  # revealed: tuple[Unknown, ...]
# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_keyword_only` is incorrect: Expected `KeywordOnlyCallback[*tuple[Unknown, ...]]`, found `def positional_only_with_keyword(x: int, y: str, /, *, flag: bool) -> None`"
reveal_type(infer_keyword_only(positional_only_with_keyword))  # revealed: tuple[Unknown, ...]
# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_keyword_only` is incorrect: Expected `KeywordOnlyCallback[*tuple[Unknown, ...]]`, found `def positional_or_keyword(x: int, y: str, flag: bool) -> None`"
reveal_type(infer_keyword_only(positional_or_keyword))  # revealed: tuple[Unknown, ...]
# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_keyword_only` is incorrect: Expected `KeywordOnlyCallback[*tuple[Unknown, ...]]`, found `def keyword_catch_all(x: int, y: str, **kwargs: object) -> None`"
reveal_type(infer_keyword_only(keyword_catch_all))  # revealed: tuple[Unknown, ...]

class OptionalKeywordCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, flag: bool = False) -> None: ...

def infer_optional_keyword[*Ts](callback: OptionalKeywordCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def optional_keyword_callback(x: int, y: str, *, flag: bool = False) -> None: ...

# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_optional_keyword` is incorrect: Expected `OptionalKeywordCallback[*tuple[Unknown, ...]]`, found `def optional_keyword_callback(x: int, y: str, *, flag: bool = False) -> None`"
reveal_type(infer_optional_keyword(optional_keyword_callback))  # revealed: tuple[Unknown, ...]

class PrefixedKeywordCallback[*Ts](Protocol):
    def __call__(self, prefix: bytes, *args: *Ts, flag: bool) -> None: ...

def infer_prefixed[*Ts](callback: PrefixedKeywordCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def prefixed(prefix: bytes, x: int, y: str, *, flag: bool) -> None: ...
def prefixed_variadic(prefix: bytes, *args: str, flag: bool) -> None: ...

# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_prefixed` is incorrect: Expected `PrefixedKeywordCallback[*tuple[Unknown, ...]]`, found `def prefixed(prefix: bytes, x: int, y: str, *, flag: bool) -> None`"
reveal_type(infer_prefixed(prefixed))  # revealed: tuple[Unknown, ...]

# An open-ended positional parameter can be inferred in an otherwise mixed signature.
reveal_type(infer_prefixed(prefixed_variadic))  # revealed: tuple[str, ...]
```

#### Variadic keyword parameters

Variadic keyword parameters are matched separately from the positional parameters captured by a
`TypeVarTuple`.

```py
from typing import Protocol

class KeywordVariadicCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, **kwargs: int) -> None: ...

def infer_keyword_variadic[*Ts](callback: KeywordVariadicCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def keyword_variadic(x: int, y: str, **kwargs: int) -> None: ...

# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_keyword_variadic` is incorrect: Expected `KeywordVariadicCallback[*tuple[Unknown, ...]]`, found `def keyword_variadic(x: int, y: str, **kwargs: int) -> None`"
reveal_type(infer_keyword_variadic(keyword_variadic))  # revealed: tuple[Unknown, ...]

class KeywordOnlyAndVariadicCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, flag: bool, **kwargs: int) -> None: ...

def infer_keyword_only_and_variadic[*Ts](
    callback: KeywordOnlyAndVariadicCallback[*Ts],
) -> tuple[*Ts]:
    raise NotImplementedError

def keyword_only_and_variadic(x: int, y: str, *, flag: bool, **kwargs: int) -> None: ...

# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_keyword_only_and_variadic` is incorrect: Expected `KeywordOnlyAndVariadicCallback[*tuple[Unknown, ...]]`, found `def keyword_only_and_variadic(x: int, y: str, *, flag: bool, **kwargs: int) -> None`"
reveal_type(infer_keyword_only_and_variadic(keyword_only_and_variadic))  # revealed: tuple[Unknown, ...]

class MultipleKeywordCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, first: int, second: str) -> None: ...

def infer_multiple_keywords[*Ts](callback: MultipleKeywordCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def multiple_keyword_catch_all(x: int, y: str, **kwargs: object) -> None: ...

# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_multiple_keywords` is incorrect: Expected `MultipleKeywordCallback[*tuple[Unknown, ...]]`, found `def multiple_keyword_catch_all(x: int, y: str, **kwargs: object) -> None`"
reveal_type(infer_multiple_keywords(multiple_keyword_catch_all))  # revealed: tuple[Unknown, ...]
```

### Length-sensitive inference

If the same `TypeVarTuple` instance is used in multiple places in a signature or class, the exact
inference behavior is not specified in the typing spec. However, all usages must match in length.

```py
def foo[*Ts](arg1: tuple[*Ts], arg2: tuple[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(foo((i, s), (b, i)))  # revealed: tuple[int, str | int]
    # error: [invalid-argument-type] "Argument to function `foo` is incorrect: Expected `tuple[int]`, found `tuple[str, bool]`"
    reveal_type(foo((i,), (s, b)))  # revealed: tuple[int]
```

## Type concatenation

A type variable tuple can be combined with fixed leading or trailing types.

```py
class Array[*Ts]: ...
class A: ...
class B: ...
class C: ...
class D: ...

def add_letter_a[*Ts](x: Array[*Ts]) -> Array[A, *Ts]:
    raise NotImplementedError

def del_letter_a[*Ts](x: Array[A, *Ts]) -> Array[*Ts]:
    raise NotImplementedError

def add_letters[*Ts](x: Array[*Ts]) -> Array[A, *Ts, C]:
    raise NotImplementedError

def del_letter_c[*Ts](x: Array[*Ts, C]) -> Array[*Ts]:
    raise NotImplementedError

def generic[T, *Ts](x: T, y: Array[*Ts]) -> Array[T, *Ts]:
    raise NotImplementedError

reveal_type(add_letters(Array[B, D]()))  # revealed: Array[A, B, D, C]
reveal_type(add_letter_a(Array[B, C]()))  # revealed: Array[A, B, C]

reveal_type(del_letter_a(Array[A, B]()))  # revealed: Array[B]
# TODO: error: [invalid-argument-type]
reveal_type(del_letter_a(Array[B, C]()))  # revealed: Array[C]

reveal_type(del_letter_c(Array[A, B, C]()))  # revealed: Array[A, B]
# TODO: error: [invalid-argument-type]
reveal_type(del_letter_c(Array[A, B]()))  # revealed: Array[A]

reveal_type(generic(A(), Array[B, D]()))  # revealed: Array[A, B, D]
reveal_type(generic(A(), Array[()]()))  # revealed: Array[A]
```

## Unpacking Unbounded Tuple Types

An unpacked unbounded tuple can describe an unknown middle section while retaining fixed endpoints,
and it can be passed into a function that solves a type variable tuple.

```py
from typing import Any

def accept_any_in_between(x: tuple[bytes, *tuple[Any, ...], int]) -> None: ...
def carry_items[*Items](x: tuple[bytes, *Items, int]) -> tuple[*Items]:
    raise NotImplementedError

def f(
    empty: tuple[bytes, int],
    multi: tuple[bytes, str, bool, int],
    truncated: tuple[bytes],
    dynamic: tuple[bytes, *tuple[Any, ...], int],
) -> None:
    accept_any_in_between(empty)
    accept_any_in_between(multi)
    # error: [invalid-argument-type] "Argument to function `accept_any_in_between` is incorrect: Expected `tuple[bytes, *tuple[Any, ...], int]`, found `tuple[bytes]`"
    accept_any_in_between(truncated)
    reveal_type(carry_items(dynamic))  # revealed: tuple[Any, ...]
```

When a mixed unbounded tuple is used to solve a `TypeVarTuple`, its fixed prefix and suffix remain
part of the solution.

```py
def preserve[*Ts](value: tuple[*Ts]) -> tuple[*Ts]:
    return value

def f(
    prefix: tuple[int, *tuple[str, ...]],
    suffix: tuple[*tuple[str, ...], bytes],
    mixed: tuple[int, *tuple[str, ...], bytes],
) -> None:
    reveal_type(preserve(prefix))  # revealed: tuple[int, *tuple[str, ...]]
    reveal_type(preserve(suffix))  # revealed: tuple[*tuple[str, ...], bytes]
    reveal_type(preserve(mixed))  # revealed: tuple[int, *tuple[str, ...], bytes]
```

A tuple containing an unpacked tuple can precisely describe heterogeneous positional arguments,
including a variable-length middle portion or a type-variable prefix.

```py
def accept_str_in_between(*args: *tuple[bool, *tuple[str, ...], bytes]) -> None: ...
def remove_bytes[*Prefix](*args: *tuple[*Prefix, bytes]) -> tuple[*Prefix]:
    raise NotImplementedError

accept_str_in_between(True, "phase", "status", b"ok")
accept_str_in_between(True, b"ok")
# error: [invalid-argument-type] "Argument to function `accept_str_in_between` is incorrect: Expected `tuple[bool, *tuple[str, ...], bytes]`"
accept_str_in_between(True, 1, b"bad")

reveal_type(remove_bytes(1, "record", b"sum"))  # revealed: tuple[Literal[1], Literal["record"]]
```

## `@staticmethod` and `@classmethod`

```py
from typing import Self

class Foo[*Ts]:
    @staticmethod
    def static_method(*args: *Ts) -> None: ...
    @classmethod
    def class_method(cls, *args: *Ts) -> Self:
        raise NotImplementedError

reveal_type(Foo[int, str].class_method(1, ""))  # revealed: Foo[int, str]

foo = Foo[int, str]()
foo.static_method(1, "")
foo.class_method(1, "")

# error: [invalid-argument-type]
foo.static_method(1, 2)
# error: [invalid-argument-type]
foo.class_method(1, 2)
```

## Type Aliases

### Variadic aliases

```py
type Simple[*Ts] = tuple[*Ts]
type Prefix[T, *Ts] = tuple[T, *Ts]
type Suffix[*Ts, T] = tuple[*Ts, T]
type Between[T, *Ts, U] = tuple[T, *Ts, U]

def _(
    a1: Simple[()],
    a2: Simple[int, str],
    a3: Between[int, str],
    a4: Between[int, bool, str],
    a5: Between[int, bool, bytes, str],
    a6: Prefix[bool],
    a7: Prefix[bool, int, str],
    a8: Suffix[bool],
    a9: Suffix[int, str, bool],
    # error: [invalid-type-arguments] "No type argument provided for required type variable `U`"
    a10: Between[int],
):
    reveal_type(a1)  # revealed: tuple[()]
    reveal_type(a2)  # revealed: tuple[int, str]
    reveal_type(a3)  # revealed: tuple[int, str]
    reveal_type(a4)  # revealed: tuple[int, bool, str]
    reveal_type(a5)  # revealed: tuple[int, bool, bytes, str]
    reveal_type(a6)  # revealed: tuple[bool]
    reveal_type(a7)  # revealed: tuple[bool, int, str]
    reveal_type(a8)  # revealed: tuple[bool]
    reveal_type(a9)  # revealed: tuple[int, str, bool]
    reveal_type(a10)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
```

### Unpacked tuple type arguments

```py
type Alias[*Ts] = tuple[int, *Ts]

def _(a1: Alias[*tuple[str, bool]], a2: Alias[*tuple[str, ...]]) -> None:
    reveal_type(a1)  # revealed: tuple[int, str, bool]
    reveal_type(a2)  # revealed: tuple[int, *tuple[str, ...]]
```

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any`, just like a bare variadic
generic class.

```py
from typing import Any

type Alias[*Fields] = tuple[bytes, *Fields]

def _(a1: Alias, a2: Alias[*tuple[Any, ...]]) -> None:
    reveal_type(a1)  # revealed: tuple[bytes, *tuple[Unknown, ...]]
    reveal_type(a2)  # revealed: tuple[bytes, *tuple[Any, ...]]
```

### Splitting arbitrary-length tuples

```py
type First[*Ts, T] = tuple[*Ts, T]
type Second[T, *Ts] = tuple[T, *Ts]

reveal_type(First[*tuple[int, ...]])  # revealed: <type alias 'First[*tuple[int, ...], int]'>
reveal_type(First[*tuple[int, ...], str])  # revealed: <type alias 'First[*tuple[int, ...], str]'>
reveal_type(Second[*tuple[int, ...]])  # revealed: <type alias 'Second[int, *tuple[int, ...]]'>
reveal_type(Second[str, *tuple[int, ...]])  # revealed: <type alias 'Second[str, *tuple[int, ...]]'>
```

### Variadic substitutions

A variadic alias can forward its remaining arguments to another variadic alias.

```py
type First[*Ts] = tuple[bytes, *Ts]
type Second[*Ts] = First[int, *Ts]

reveal_type(First[str, bool])  # revealed: <type alias 'First[str, bool]'>
reveal_type(Second[str, bool])  # revealed: <type alias 'Second[str, bool]'>
```

### Using Callable

```py
from typing import Callable

type Alias[*Ts] = Callable[[*Ts], None]

def test[*Ts](fn: Alias[int, *Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def fn0(a: int) -> None: ...
def fn1(a: int, b: str) -> None: ...
def fn2(a: int, b: str, c: bytes) -> None: ...

# TODO: Should reveal `tuple[()]` without an error.
# error: [invalid-argument-type] "Argument to function `test` is incorrect: Expected `Alias[*tuple[int, *tuple[Unknown, ...]]]`, found `def fn0(a: int) -> None`"
reveal_type(test(fn0))  # revealed: tuple[Unknown, ...]
# TODO: Should reveal `tuple[str]` without an error.
# error: [invalid-argument-type] "Argument to function `test` is incorrect: Expected `Alias[*tuple[int, *tuple[Unknown, ...]]]`, found `def fn1(a: int, b: str) -> None`"
reveal_type(test(fn1))  # revealed: tuple[Unknown, ...]
# TODO: Should reveal `tuple[str, bytes]` without an error.
# error: [invalid-argument-type] "Argument to function `test` is incorrect: Expected `Alias[*tuple[int, *tuple[Unknown, ...]]]`, found `def fn2(a: int, b: str, c: bytes) -> None`"
reveal_type(test(fn2))  # revealed: tuple[Unknown, ...]
```

## Accessing Individual Types

Operations that need to rearrange individual members of a type variable tuple can expose overloads
for each supported tuple length.

```py
from typing import Any, overload

class Row[*Cells]:
    @overload
    def get[A, B](self: "Row[A, B]") -> "Row[B, A]": ...
    @overload
    def get[A, B, C](self: "Row[A, B, C]") -> "Row[B, C, A]": ...
    def get(self) -> "Row[*tuple[Any, ...]]":
        raise NotImplementedError

def f(pair: Row[int, str], triple: Row[int, str, bytes]) -> None:
    reveal_type(pair.get())  # revealed: Row[str, int]
    # TODO: Should reveal `Row[str, bytes, int]`.
    reveal_type(triple.get())  # revealed: Row[Unknown, Unknown]
```

## Invalid Forms

### Multiple Type Variable Tuples not allowed

Only one type variable tuple can appear in a type parameter list.

```py
# error: [invalid-type-form]
class Array[*Ts1, *Ts2]: ...
```

### Must always be unpacked

```py
def invalid[*Ts](x: Ts) -> None: ...  # error: [invalid-type-form]
def invalid_args[*Ts](*args: Ts) -> None: ...  # error: [invalid-type-form]

class InvalidTupleElement[*Ts]:
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts` is not valid in this context in a type expression"
    values: tuple[Ts]

def valid[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    return x
```

### Invalid unpack operand

Only tuple types and type variable tuples can be unpacked in a type expression.

```py
# error: [invalid-type-form] "`*` can only unpack a tuple type or `TypeVarTuple`"
def invalid(*args: *int) -> None:
    reveal_type(args)  # revealed: tuple[Unknown, ...]

class Pair[*Ts, U]: ...

def invalid_generic(
    # error: [invalid-type-form] "`*` can only unpack a tuple type or `TypeVarTuple`"
    value: Pair[*int, str],
) -> None:
    reveal_type(value)  # revealed: Pair[*tuple[Unknown, ...], str]
```

### Only one variadic unpack

```py
def f[*Ts](
    ok1: tuple[int, *Ts],
    ok2: tuple[int, *Ts, str],
    bad1: tuple[*Ts, *tuple[str, ...]],  # error: [invalid-type-form]
    bad2: tuple[*tuple[str, ...], *Ts],  # error: [invalid-type-form]
) -> None: ...
```
