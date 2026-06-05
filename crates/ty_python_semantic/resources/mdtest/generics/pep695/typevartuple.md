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

### Assignment checks

```py
class Array[*Ts]:
    values: tuple[*Ts]

def takes_int_str(x: Array[int, str]) -> None: ...
def takes_int_str_tuple(x: tuple[int, str]) -> None: ...
def f(x: Array[int, str], y: Array[str, int], xs: tuple[int, str], ys: tuple[str, int]) -> None:
    takes_int_str(x)
    takes_int_str(y)  # error: [invalid-argument-type]
    takes_int_str_tuple(xs)
    takes_int_str_tuple(ys)  # error: [invalid-argument-type]
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

Container = TypeVar("Container", Packed[*tuple[Any, ...]], Scalar)

def operate(value: Container) -> None:
    value.operate()

def clone(value: Container) -> Container:
    reveal_type(value.clone())  # revealed: Container@clone
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
    reveal_type(simple(()))  # revealed: tuple[()]
    reveal_type(simple((i, s)))  # revealed: tuple[int, str]
    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix((i, s), b))  # revealed: tuple[int, str, bool]
    reveal_type(with_both((i, s, b)))  # revealed: tuple[str]
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

def kw_only[T, *Ts](*args: *Ts, kw: T) -> tuple[*Ts, T]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(simple())  # revealed: tuple[()]
    reveal_type(simple(i, s))  # revealed: tuple[int, str]
    reveal_type(with_prefix(i, s, b))  # revealed: tuple[int, str, bool]
    reveal_type(kw_only(i, s, kw=b))  # revealed: tuple[int, str, bool]
    # error: [missing-argument] "No argument provided for required parameter `kw` of function `kw_only`"
    reveal_type(kw_only(i, s, b))  # revealed: tuple[int, str, bool, Unknown]
```

### Callable parameters

`Callable` accepts unpacked `TypeVarTuple`s in its positional parameter list.

```py
from typing import Any, Callable

def simple[*Ts](callback: Callable[[*Ts], tuple[*Ts]]) -> tuple[*Ts]:
    reveal_type(callback)  # revealed: (*Ts@simple) -> tuple[*Ts@simple]
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

reveal_type(simple(positional_only))  # revealed: tuple[int, str]
reveal_type(simple(standard))  # revealed: tuple[int, str]
reveal_type(simple(positional_variadic))  # revealed: tuple[int, *tuple[str, ...]]
reveal_type(simple(variadic1))  # revealed: tuple[int, ...]

# error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `(*args: int) -> tuple[int, ...]`, found `def variadic2(*args: int) -> tuple[str, ...]`"
reveal_type(simple(variadic2))  # revealed: tuple[int, ...]
# error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `(*args: Unknown) -> tuple[Unknown, ...]`, found `def keyword_only(*, x: int) -> tuple[int]`"
reveal_type(simple(keyword_only))  # revealed: tuple[Unknown, ...]
```

This usage pattern is similar to how `ParamSpec` can be used to accept a callable and its arguments
except that in the case of `TypeVarTuple` all parameters are positional-only.

```py
def invoke[*Ts, R](callback: Callable[[*Ts], R], *args: *Ts) -> R:
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

type Headered[*Fields] = tuple[bytes, *Fields]

def _(a1: Headered, a2: Headered[*tuple[Any, ...]]) -> None:
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

## Accessing Individual Types

Operations that need to rearrange individual members of a type variable tuple can expose overloads
for each supported tuple length.

```py
from typing import overload

class Row[*Cells]:
    def cells(self) -> tuple[*Cells]:
        raise NotImplementedError

    @overload
    def rotate_left[A, B](self: "Row[A, B]") -> "Row[B, A]": ...
    @overload
    def rotate_left[A, B, C](self: "Row[A, B, C]") -> "Row[B, C, A]": ...
    def rotate_left(self) -> "Row":
        raise NotImplementedError

def f(pair: Row[int, str], triple: Row[int, str, bytes]) -> None:
    reveal_type(pair.rotate_left())  # revealed: Row[str, int]
    reveal_type(triple.rotate_left())  # revealed: Row[str, bytes, int]
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

### Only one variadic unpack

```py
def f[*Ts](
    ok1: tuple[int, *Ts],
    ok2: tuple[int, *Ts, str],
    bad1: tuple[*Ts, *tuple[str, ...]],  # error: [invalid-type-form]
    bad2: tuple[*tuple[str, ...], *Ts],  # error: [invalid-type-form]
) -> None: ...
```
