# PEP 695 `TypeVarTuple`

```toml
[environment]
python-version = "3.12"
```

## Definition

A PEP 695 type variable tuple is introduced with a single starred type parameter.

```py
def foo[*Ts](*args: *Ts) -> None:
    # TODO: revealed: TypeVarTuple
    reveal_type(Ts)  # revealed: @Todo(PEP-695 TypeVarTuple definition types)
    # TODO: revealed: tuple[*Ts@foo]
    reveal_type(args)  # revealed: @Todo(PEP 646)
```

## Variance inference

PEP 695 type variable tuples infer variance from how the class uses them.

```py
class CovariantArray[*Ts]:
    def get(self) -> tuple[*Ts]:
        raise NotImplementedError

# error: [not-subscriptable]
# error: [not-subscriptable]
covariant_ok: CovariantArray[object] = CovariantArray[int]()
# error: [not-subscriptable]
# error: [not-subscriptable]
covariant_error: CovariantArray[int] = CovariantArray[object]()  # TODO: error: [invalid-assignment]

class ContravariantArray[*Ts]:
    def set(self, value: tuple[*Ts]) -> None:
        raise NotImplementedError

# error: [not-subscriptable]
# error: [not-subscriptable]
contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
# error: [not-subscriptable]
# error: [not-subscriptable]
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # TODO: error: [invalid-assignment]

class InvariantArray[*Ts]:
    values: tuple[*Ts]

# error: [not-subscriptable]
# error: [not-subscriptable]
invariant_out: InvariantArray[object] = InvariantArray[int]()  # TODO: error: [invalid-assignment]
# error: [not-subscriptable]
# error: [not-subscriptable]
invariant_in: InvariantArray[int] = InvariantArray[object]()  # TODO: error: [invalid-assignment]
```

## Generic Classes

### Explicit specialization

```py
class Simple[*Ts]:
    attr: tuple[*Ts]

# TODO: revealed: tuple[()]
reveal_type(Simple[()]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, str]
# error: [not-subscriptable]
reveal_type(Simple[int, str]().attr)  # revealed: Unknown
# TODO: revealed: tuple[int, str]
# error: [not-subscriptable]
reveal_type(Simple[*tuple[int, str]]().attr)  # revealed: Unknown

# TODO: error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
# TODO: revealed: tuple[Unknown]
# error: [not-subscriptable]
reveal_type(Simple[[int, str]]().attr)  # revealed: Unknown
# TODO: error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
# TODO: revealed: tuple[Unknown, ...]
# error: [not-subscriptable]
reveal_type(Simple[*[int, str]]().attr)  # revealed: Unknown
```

```py
class Prefix[T, *Ts]:
    attr: tuple[T, *Ts]

# TODO: revealed: tuple[int]
reveal_type(Prefix[int]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, bool]
# error: [invalid-type-arguments]
reveal_type(Prefix[int, bool]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, bool, str]
# error: [invalid-type-arguments]
reveal_type(Prefix[int, bool, str]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, bool, str]
# error: [invalid-type-arguments]
reveal_type(Prefix[int, *tuple[bool, str]]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]

# TODO: Should this raise an error?
# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...]]
reveal_type(Prefix().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

```py
class Suffix[*Ts, T]:
    attr: tuple[*Ts, T]

# TODO: revealed: tuple[int]
reveal_type(Suffix[int]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, str]
# error: [invalid-type-arguments]
reveal_type(Suffix[int, str]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, str, bool]
# error: [invalid-type-arguments]
reveal_type(Suffix[int, str, bool]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, str, bool]
# error: [invalid-type-arguments]
reveal_type(Suffix[*tuple[int, str], bool]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]

# TODO: Should this raise an error?
# TODO: revealed: tuple[*tuple[Unknown, ...], Unknown]
reveal_type(Suffix().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

```py
class Between[T, *Ts, U]:
    attr: tuple[T, *Ts, U]

# TODO: revealed: tuple[int, str]
reveal_type(Between[int, str]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, bool, str]
# error: [invalid-type-arguments]
reveal_type(Between[int, bool, str]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, bool, bytes, str]
# error: [invalid-type-arguments]
reveal_type(Between[int, bool, bytes, str]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, bool, str]
# error: [invalid-type-arguments]
reveal_type(Between[int, *tuple[bool], str]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]

# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
reveal_type(Between().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
# error: [invalid-type-arguments] "No type argument provided for required type variable `U` of class `Between`"
reveal_type(Between[int]().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
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

# TODO: revealed: Positional[()]
reveal_type(Positional(()))  # revealed: Positional[]
# TODO: revealed: Positional[int, str]
reveal_type(Positional((1, "a")))  # revealed: Positional[]

# TODO: revealed: Variadic[()]
reveal_type(Variadic())  # revealed: Variadic[]
# TODO: revealed: Variadic[int, str]
reveal_type(Variadic(1, "a"))  # revealed: Variadic[]

def _(i: int, s: str) -> None:
    # TODO: revealed: Positional[int, str]
    reveal_type(Positional((i, s)))  # revealed: Positional[]
    # TODO: revealed: Variadic[int, str]
    reveal_type(Variadic(i, s))  # revealed: Variadic[]
```

### Unspecified type arguments

An unsubscripted variadic generic behaves as if it used an unknown-length tuple of `Any` arguments.
ty represents the missing type information as `Unknown`, distinguishing it from explicitly provided
`Any`.

```py
class Unspecified[*Ts]:
    attr: tuple[*Ts]

unspecified = Unspecified()
# TODO: revealed: Unspecified[*tuple[Unknown, ...]]
reveal_type(unspecified)  # revealed: Unspecified[]
# TODO: revealed: tuple[Unknown, ...]
reveal_type(unspecified.attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
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

# TODO: revealed: tuple[int, str]
reveal_type(WithDefault().attr)  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[bool, bytes]
# error: [not-subscriptable]
reveal_type(WithDefault[bool, bytes]().attr)  # revealed: Unknown
```

### Assignment checks

```py
class Array[*Ts]:
    values: tuple[*Ts]

# error: [not-subscriptable]
def takes_int_str(x: Array[int, str]) -> None: ...
def takes_int_str_tuple(x: tuple[int, str]) -> None: ...

# error: [not-subscriptable]
# error: [not-subscriptable]
def f(x: Array[int, str], y: Array[str, int], xs: tuple[int, str], ys: tuple[str, int]) -> None:
    takes_int_str(x)
    takes_int_str(y)  # TODO: error: [invalid-argument-type]
    takes_int_str_tuple(xs)
    takes_int_str_tuple(ys)  # error: [invalid-argument-type]
```

### Gradual specializations

A type variable tuple remains assignable to an explicitly gradual specialization of its generic
class.

```py
from typing import Any

class Array[*Ts]:
    # error: [not-subscriptable]
    def erase_shape(self) -> "Array[*tuple[Any, ...]]":
        return self
```

A constrained type variable preserves the correlation between each constraint and the bound method's
implicit `self` argument.

```py
from typing import Any, Self, TypeVar

class Packed[*Ts]:
    def operate(self) -> None: ...
    def clone(self) -> Self:
        raise NotImplementedError

class Scalar:
    def operate(self) -> None: ...
    def clone(self) -> Self:
        raise NotImplementedError

# error: [not-subscriptable]
Container = TypeVar("Container", Packed[*tuple[Any, ...]], Scalar)

def operate(value: Container) -> None:
    value.operate()

def clone(value: Container) -> Container:
    # TODO: revealed: Container@clone
    reveal_type(value.clone())  # revealed: Unknown | Container@clone
    return value.clone()
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

def with_both[T, *Ts, U](x: tuple[T, *Ts, U]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    # TODO: revealed: tuple[()]
    reveal_type(simple(()))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str]
    reveal_type(simple((i, s)))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(with_suffix((i, s), b))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[str]
    reveal_type(with_both((i, s, b)))  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

### Starred variadic parameters

When a `TypeVarTuple` appears in `*args`, the argument tuple keeps the exact element types from the
call site.

```py
def simple[*Ts](*args: *Ts) -> tuple[*Ts]:
    # TODO: revealed: tuple[*Ts@simple]
    reveal_type(args)  # revealed: @Todo(PEP 646)
    raise NotImplementedError

def with_prefix[T, *Ts](prefix: T, *args: *Ts) -> tuple[T, *Ts]:
    raise NotImplementedError

def kw_only[T, *Ts](*args: *Ts, kw: T) -> tuple[*Ts, T]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    # TODO: revealed: tuple[()]
    reveal_type(simple())  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str]
    reveal_type(simple(i, s))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(with_prefix(i, s, b))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(kw_only(i, s, kw=b))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str, bool, Unknown]
    # error: [missing-argument] "No argument provided for required parameter `kw` of function `kw_only`"
    reveal_type(kw_only(i, s, b))  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

### Callable parameters

`Callable` accepts unpacked `TypeVarTuple`s in its positional parameter list.

```py
from typing import Any, Callable

def simple[*Ts](callback: Callable[[*Ts], tuple[*Ts]]) -> tuple[*Ts]:
    # TODO: revealed: (*Ts@simple) -> tuple[*Ts@simple]
    reveal_type(callback)  # revealed: (...) -> tuple[@Todo(TypeVarTuple), ...]
    raise NotImplementedError

def positional_only(x: int, y: str, /) -> tuple[int, str]:
    raise NotImplementedError

def standard(x: int, y: str) -> tuple[int, str]:
    raise NotImplementedError

def positional_variadic(x: int, *args: str) -> tuple[int, *tuple[str, ...]]:
    raise NotImplementedError

def variadic1(*args: int) -> tuple[int, ...]:
    raise NotImplementedError

def variadic2(*args: int) -> tuple[str, ...]:
    raise NotImplementedError

def keyword_only(*, x: int) -> tuple[int]:
    raise NotImplementedError

# TODO: revealed: tuple[int, str]
reveal_type(simple(positional_only))  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, str]
reveal_type(simple(standard))  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, *tuple[str, ...]]
reveal_type(simple(positional_variadic))  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: revealed: tuple[int, ...]
reveal_type(simple(variadic1))  # revealed: tuple[@Todo(TypeVarTuple), ...]

# TODO: error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `(*args: int) -> tuple[int, ...]`, found `def variadic2(*args: int) -> tuple[str, ...]`"
# TODO: revealed: tuple[int, ...]
reveal_type(simple(variadic2))  # revealed: tuple[@Todo(TypeVarTuple), ...]
# TODO: error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `(*args: Unknown) -> tuple[Unknown, ...]`, found `def keyword_only(*, x: int) -> tuple[int]`"
# TODO: revealed: tuple[Unknown, ...]
reveal_type(simple(keyword_only))  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

This usage pattern is similar to how `ParamSpec` can be used to accept a callable and its arguments
except that in the case of `TypeVarTuple` all parameters are positional-only.

```py
def invoke[*Ts, R](callback: Callable[[*Ts], R], *args: *Ts) -> R:
    raise NotImplementedError

reveal_type(invoke(positional_only, 1, "a"))  # revealed: tuple[int, str]
# TODO: error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `() -> tuple[int, str]`, found `def positional_only(x: int, y: str, /) -> tuple[int, str]`"
reveal_type(invoke(positional_only))  # revealed: tuple[int, str]
# TODO: error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `(Literal[1], /) -> tuple[int, str]`, found `def positional_only(x: int, y: str, /) -> tuple[int, str]`"
reveal_type(invoke(positional_only, 1))  # revealed: tuple[int, str]
# TODO: error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `(int, Literal[2] | str, /) -> tuple[int, str]`, found `def positional_only(x: int, y: str, /) -> tuple[int, str]`"
reveal_type(invoke(positional_only, 1, 2))  # revealed: tuple[int, str]

reveal_type(invoke(standard, 1, "a"))  # revealed: tuple[int, str]
# TODO: error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `() -> tuple[int, str]`, found `def standard(x: int, y: str) -> tuple[int, str]`"
# error: [unknown-argument] "Argument `x` does not match any known parameter of function `invoke`"
# error: [unknown-argument] "Argument `y` does not match any known parameter of function `invoke`"
reveal_type(invoke(standard, x=1, y="a"))  # revealed: tuple[int, str]

reveal_type(invoke(positional_variadic, 1, "a", "b"))  # revealed: tuple[int, *tuple[str, ...]]
reveal_type(invoke(positional_variadic, 1))  # revealed: tuple[int, *tuple[str, ...]]
# TODO: error: [invalid-argument-type] "Argument to function `invoke` is incorrect: Expected `() -> tuple[int, *tuple[str, ...]]`, found `def positional_variadic(x: int, *args: str) -> tuple[int, *tuple[str, ...]]`"
reveal_type(invoke(positional_variadic))  # revealed: tuple[int, *tuple[str, ...]]
```

### Length-sensitive inference

If the same `TypeVarTuple` instance is used in multiple places in a signature or class, the exact
inference behavior is not specified in the typing spec. However, all usages must match in length.

```py
def foo[*Ts](arg1: tuple[*Ts], arg2: tuple[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    # TODO: revealed: tuple[int, str | int]
    reveal_type(foo((i, s), (b, i)))  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: error: [invalid-argument-type] "Argument to function `foo` is incorrect: Expected `tuple[int]`, found `tuple[str, bool]`"
    # TODO: revealed: tuple[int]
    reveal_type(foo((i,), (s, b)))  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

## Type concatenation

A type variable tuple can be combined with fixed leading or trailing types.

```py
class Array[*Ts]: ...
class A: ...
class B: ...
class C: ...
class D: ...

# error: [not-subscriptable]
# error: [not-subscriptable]
def add_letter_a[*Ts](x: Array[*Ts]) -> Array[A, *Ts]:
    raise NotImplementedError

# error: [not-subscriptable]
# error: [not-subscriptable]
def del_letter_a[*Ts](x: Array[A, *Ts]) -> Array[*Ts]:
    raise NotImplementedError

# error: [not-subscriptable]
# error: [not-subscriptable]
def add_letters[*Ts](x: Array[*Ts]) -> Array[A, *Ts, C]:
    raise NotImplementedError

# error: [not-subscriptable]
# error: [not-subscriptable]
def del_letter_c[*Ts](x: Array[*Ts, C]) -> Array[*Ts]:
    raise NotImplementedError

# error: [not-subscriptable]
# error: [not-subscriptable]
def generic[T, *Ts](x: T, y: Array[*Ts]) -> Array[T, *Ts]:
    raise NotImplementedError

# TODO: revealed: Array[A, B, D, C]
# error: [not-subscriptable]
reveal_type(add_letters(Array[B, D]()))  # revealed: Unknown
# TODO: revealed: Array[A, B, C]
# error: [not-subscriptable]
reveal_type(add_letter_a(Array[B, C]()))  # revealed: Unknown

# TODO: revealed: Array[B]
# error: [not-subscriptable]
reveal_type(del_letter_a(Array[A, B]()))  # revealed: Unknown
# TODO: error: [invalid-argument-type]
# TODO: revealed: Array[C]
# error: [not-subscriptable]
reveal_type(del_letter_a(Array[B, C]()))  # revealed: Unknown

# TODO: revealed: Array[A, B]
# error: [not-subscriptable]
reveal_type(del_letter_c(Array[A, B, C]()))  # revealed: Unknown
# TODO: error: [invalid-argument-type]
# TODO: revealed: Array[A]
# error: [not-subscriptable]
reveal_type(del_letter_c(Array[A, B]()))  # revealed: Unknown

# TODO: revealed: Array[A, B, D]
# error: [not-subscriptable]
reveal_type(generic(A(), Array[B, D]()))  # revealed: Unknown
# TODO: revealed: Array[A]
reveal_type(generic(A(), Array[()]()))  # revealed: Unknown
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
    # TODO: revealed: tuple[Any, ...]
    reveal_type(carry_items(dynamic))  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

A tuple containing an unpacked tuple can precisely describe heterogeneous positional arguments,
including a variable-length middle portion or a type-variable prefix.

```py
def accept_str_in_between(*args: *tuple[bool, *tuple[str, ...], bytes]) -> None: ...
def remove_bytes[*Prefix](*args: *tuple[*Prefix, bytes]) -> tuple[*Prefix]:
    raise NotImplementedError

accept_str_in_between(True, "phase", "status", b"ok")
accept_str_in_between(True, b"ok")
# TODO: error: [invalid-argument-type] "Argument to function `accept_str_in_between` is incorrect: Expected `tuple[bool, *tuple[str, ...], bytes]`"
accept_str_in_between(True, 1, b"bad")

# TODO: revealed: tuple[Literal[1], Literal["record"]]
reveal_type(remove_bytes(1, "record", b"sum"))  # revealed: tuple[@Todo(TypeVarTuple), ...]
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
    # error: [not-subscriptable]
    a2: Simple[int, str],
    a3: Between[int, str],
    # error: [invalid-type-arguments]
    a4: Between[int, bool, str],
    # error: [invalid-type-arguments]
    a5: Between[int, bool, bytes, str],
    a6: Prefix[bool],
    # error: [invalid-type-arguments]
    a7: Prefix[bool, int, str],
    a8: Suffix[bool],
    # error: [invalid-type-arguments]
    a9: Suffix[int, str, bool],
    # error: [invalid-type-arguments] "No type argument provided for required type variable `U`"
    a10: Between[int],
):
    # TODO: revealed: tuple[()]
    reveal_type(a1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str]
    reveal_type(a2)  # revealed: Unknown
    # TODO: revealed: tuple[int, str]
    reveal_type(a3)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, bool, str]
    reveal_type(a4)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, bool, bytes, str]
    reveal_type(a5)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bool]
    reveal_type(a6)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bool, int, str]
    reveal_type(a7)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bool]
    reveal_type(a8)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(a9)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
    reveal_type(a10)  # revealed: tuple[@Todo(TypeVarTuple), ...]
```

### Unpacked tuple type arguments

```py
type Alias[*Ts] = tuple[int, *Ts]

# error: [not-subscriptable]
# error: [not-subscriptable]
def _(a1: Alias[*tuple[str, bool]], a2: Alias[*tuple[str, ...]]) -> None:
    # TODO: revealed: tuple[int, str, bool]
    reveal_type(a1)  # revealed: Unknown
    # TODO: revealed: tuple[int, *tuple[str, ...]]
    reveal_type(a2)  # revealed: Unknown
```

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any`, just like a bare variadic
generic class.

```py
from typing import Any

type Headered[*Fields] = tuple[bytes, *Fields]

# error: [not-subscriptable]
def _(a1: Headered, a2: Headered[*tuple[Any, ...]]) -> None:
    # TODO: revealed: tuple[bytes, *tuple[Unknown, ...]]
    reveal_type(a1)  # revealed: tuple[@Todo(TypeVarTuple), ...]
    # TODO: revealed: tuple[bytes, *tuple[Any, ...]]
    reveal_type(a2)  # revealed: Unknown
```

### Splitting arbitrary-length tuples

```py
type First[*Ts, T] = tuple[*Ts, T]
type Second[T, *Ts] = tuple[T, *Ts]

# TODO: revealed: <type alias 'First[*tuple[int, ...], int]'>
reveal_type(First[*tuple[int, ...]])  # revealed: <type alias 'First[tuple[int, ...]]'>
# TODO: revealed: <type alias 'First[*tuple[int, ...], str]'>
# error: [invalid-type-arguments]
reveal_type(First[*tuple[int, ...], str])  # revealed: <type alias 'First[Unknown]'>
# TODO: revealed: <type alias 'Second[int, *tuple[int, ...]]'>
reveal_type(Second[*tuple[int, ...]])  # revealed: <type alias 'Second[tuple[int, ...]]'>
# TODO: revealed: <type alias 'Second[str, *tuple[int, ...]]'>
# error: [invalid-type-arguments]
reveal_type(Second[str, *tuple[int, ...]])  # revealed: <type alias 'Second[Unknown]'>
```

### Variadic substitutions

A variadic alias can forward its remaining arguments to another variadic alias.

```py
type First[*Ts] = tuple[bytes, *Ts]
# error: [not-subscriptable]
type Second[*Ts] = First[int, *Ts]

# TODO: revealed: <type alias 'First[str, bool]'>
# error: [not-subscriptable]
reveal_type(First[str, bool])  # revealed: Unknown
# TODO: revealed: <type alias 'Second[str, bool]'>
# error: [not-subscriptable]
reveal_type(Second[str, bool])  # revealed: Unknown
```

## Accessing Individual Types

Operations that need to rearrange individual members of a type variable tuple can expose overloads
for each supported tuple length.

```py
from typing import Any, overload

class Row[*Cells]:
    def cells(self) -> tuple[*Cells]:
        raise NotImplementedError

    @overload
    # error: [not-subscriptable]
    # error: [not-subscriptable]
    def rotate_left[A, B](self: "Row[A, B]") -> "Row[B, A]": ...
    @overload
    # error: [not-subscriptable]
    # error: [not-subscriptable]
    def rotate_left[A, B, C](self: "Row[A, B, C]") -> "Row[B, C, A]": ...
    # error: [not-subscriptable]
    def rotate_left(self) -> "Row[*tuple[Any, ...]]":
        raise NotImplementedError

# error: [not-subscriptable]
# error: [not-subscriptable]
def f(pair: Row[int, str], triple: Row[int, str, bytes]) -> None:
    # TODO: revealed: Row[str, int]
    reveal_type(pair.rotate_left())  # revealed: Unknown
    # TODO: revealed: Row[str, bytes, int]
    reveal_type(triple.rotate_left())  # revealed: Unknown
```

## Invalid Forms

### Multiple Type Variable Tuples not allowed

Only one type variable tuple can appear in a type parameter list.

```py
# TODO: error: [invalid-type-form]
class Array[*Ts1, *Ts2]: ...
```

### Must always be unpacked

```py
def invalid[*Ts](x: Ts) -> None: ...  # TODO: error: [invalid-type-form]
def invalid_args[*Ts](*args: Ts) -> None: ...  # TODO: error: [invalid-type-form]

class InvalidTupleElement[*Ts]:
    # TODO: error: [invalid-type-form] "Bare TypeVarTuple `Ts` is not valid in this context in a type expression"
    values: tuple[Ts]

def valid[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    return x
```

### Invalid unpack operand

Only tuple types and type variable tuples can be unpacked in a type expression.

```py
# TODO: error: [invalid-type-form] "`*` can only unpack a tuple type or `TypeVarTuple`"
def invalid(*args: *int) -> None:
    # TODO: revealed: tuple[Unknown, ...]
    reveal_type(args)  # revealed: @Todo(PEP 646)

class Pair[*Ts, U]: ...

def invalid_generic(
    # TODO: error: [invalid-type-form] "`*` can only unpack a tuple type or `TypeVarTuple`"
    # error: [invalid-type-arguments]
    value: Pair[*int, str],
) -> None:
    # TODO: revealed: Pair[*tuple[Unknown, ...], str]
    reveal_type(value)  # revealed: Pair[Unknown]
```

### Only one variadic unpack

```py
def f[*Ts](
    ok1: tuple[int, *Ts],
    ok2: tuple[int, *Ts, str],
    bad1: tuple[*Ts, *tuple[str, ...]],  # TODO: error: [invalid-type-form]
    bad2: tuple[*tuple[str, ...], *Ts],  # TODO: error: [invalid-type-form]
) -> None: ...
```
