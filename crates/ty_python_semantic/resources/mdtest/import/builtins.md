# Builtins

## Importing builtin module

Builtin symbols can be explicitly imported:

```py
import builtins

reveal_type(builtins.chr)  # revealed: def chr(i: SupportsIndex, /) -> str
```

## Implicit use of builtin

Or used implicitly:

```py
reveal_type(chr)  # revealed: def chr(i: SupportsIndex, /) -> str
reveal_type(str)  # revealed: <class 'str'>
```

## Private type-checking-only builtin helpers are not implicit builtins

Private type variables, type aliases, and type-checking-only definitions in a `builtins` stub are
implementation details. They must not be available without an explicit import.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/typing.pyi`:

```pyi
class TypeVar:
    def __new__(cls, name): ...

class ParamSpec:
    def __new__(cls, name): ...

class Protocol: ...
class _SpecialForm: ...

TypeAlias: _SpecialForm

def type_check_only(obj): ...
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
from typing import ParamSpec, Protocol, TypeAlias, TypeVar, type_check_only

class object: ...
class int: ...

_T = TypeVar("_T")
_P = ParamSpec("_P")
_PrivateAlias: TypeAlias = int

@type_check_only
class _PrivateProtocol(Protocol): ...

@type_check_only
class PublicTypeOnlyClass: ...

@type_check_only
def public_type_only_function(): ...
```

`module.py`:

```py
_T  # error: [unresolved-reference]
_P  # error: [unresolved-reference]
_PrivateAlias  # error: [unresolved-reference]
_PrivateProtocol  # error: [unresolved-reference]
PublicTypeOnlyClass  # error: [unresolved-reference]
public_type_only_function  # error: [unresolved-reference]
```

## Explicitly importing private builtin helpers

We still allow users to explicitly import implementation details from the `builtins` module.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/typing.pyi`:

```pyi
class TypeVar:
    def __new__(cls, name): ...

class Protocol: ...
class _SpecialForm: ...

TypeAlias: _SpecialForm

def type_check_only(obj): ...
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
from typing import Protocol, TypeAlias, TypeVar, type_check_only

class object: ...
class int: ...

_T = TypeVar("_T")
_PrivateAlias: TypeAlias = int

@type_check_only
class _PrivateProtocol(Protocol): ...

@type_check_only
class PublicTypeOnlyClass: ...
```

`module.py`:

```py
from builtins import PublicTypeOnlyClass, _PrivateAlias, _PrivateProtocol, _T

_T
_PrivateAlias
_PrivateProtocol
PublicTypeOnlyClass
```

## Private project-level builtins

A project-level `__builtins__.pyi` can deliberately provide private runtime names, including names
that overlap with private helpers in the standard `builtins` stub.

```py
reveal_type(_private_value)  # revealed: int
reveal_type(_T_co)  # revealed: int

_PrivateTypeVar  # error: [unresolved-reference]
_PrivateAlias  # error: [unresolved-reference]
_PrivateTypeOnlyProtocol  # error: [unresolved-reference]
_PrivateTypeCheckingProtocol  # error: [unresolved-reference]

_RuntimeProtocol
_runtime_typevar
```

`__builtins__.pyi`:

```pyi
from typing import TYPE_CHECKING, Protocol, TypeAlias, TypeVar, type_check_only

_private_value: int
_T_co: int

_PrivateTypeVar = TypeVar("_PrivateTypeVar")
_PrivateAlias: TypeAlias = int

@type_check_only
class _PrivateTypeOnlyProtocol(Protocol): ...

if TYPE_CHECKING:
    class _PrivateTypeCheckingProtocol(Protocol): ...

class _RuntimeProtocol(Protocol): ...

def make_typevar() -> TypeVar: ...

_runtime_typevar = make_typevar()
```

## Private type-checking-only builtins with stacked decorators

An outer decorator can change the inferred type of a private function or class, but it does not make
an inner `@type_check_only` definition available at runtime.

```py
_PrivateFunction  # error: [unresolved-reference]
_PrivateClass  # error: [unresolved-reference]
```

`__builtins__.pyi`:

```pyi
from typing import Callable, type_check_only

def decorate_function(callback: Callable[[int], int]) -> Callable[[int], int]: ...
def decorate_class(cls: type[object]) -> type[object]: ...
@decorate_function
@type_check_only
def _PrivateFunction(value: int) -> int: ...

@decorate_class
@type_check_only
class _PrivateClass: ...
```

## Private runtime standard builtins

A private class declared by the standard `builtins` stub remains available when it represents a real
runtime builtin, rather than a type-checking-only helper.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class _IncompleteInputError: ...
```

`module.py`:

```py
_IncompleteInputError
```

## Builtin symbol from custom typeshed

If we specify a custom typeshed, we can use the builtin symbol from it, and no longer access the
builtins from the "actual" vendored typeshed:

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class Custom: ...

custom_builtin: Custom
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

```py
reveal_type(custom_builtin)  # revealed: Custom

# error: [unresolved-reference]
reveal_type(str)  # revealed: Unknown
```

## Unknown builtin (later defined)

`foo` has a type of `Unknown` in this example, as it relies on `bar` which has not been defined at
that point:

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
foo = bar
bar = 1
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

```py
reveal_type(foo)  # revealed: Unknown
```

## Builtins imported from custom project-level stubs

The project can add or replace builtins with the `__builtins__.pyi` stub. They will take precedence
over the typeshed ones.

```py
reveal_type(foo)  # revealed: int
reveal_type(bar)  # revealed: str
reveal_type(quux(1))  # revealed: int
b = baz  # error: [unresolved-reference]

reveal_type(ord(100))  # revealed: bool
a = ord("a")  # error: [invalid-argument-type]

bar = int(123)
reveal_type(bar)  # revealed: int
```

`__builtins__.pyi`:

```pyi
foo: int = ...
bar: str = ...

def quux(value: int) -> int: ...

unused: str = ...

def ord(x: int) -> bool: ...
```

Builtins stubs are searched relative to the project root, not the file using them.

`under/some/folder.py`:

```py
reveal_type(foo)  # revealed: int
reveal_type(bar)  # revealed: str
```

## Assigning custom builtins

```py
import builtins

builtins.foo = 123
builtins.bar = 456  # error: [unresolved-attribute]
builtins.baz = 789  # error: [invalid-assignment]
builtins.chr = lambda x: str(x)  # error: [invalid-assignment]
builtins.chr = 10
```

`__builtins__.pyi`:

```pyi
foo: int
baz: str
chr: int
```
