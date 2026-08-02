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

## Stub-only symbols are not implicit builtins

Private type variables, parameter specifications, type aliases, and protocols in `builtins.pyi` are
implementation details of the stub. They must not be available without an explicit import.

```py
_T_co  # error: [unresolved-reference]
_P  # error: [unresolved-reference]
_PositiveInteger  # error: [unresolved-reference]
_Opener  # error: [unresolved-reference]
_SupportsSynchronousAnext  # error: [unresolved-reference]
```

## Explicitly importing stub-only builtin symbols

An explicit import can still access private helpers from `builtins.pyi`, just as it can access
private symbols from any other stub.

```py
from builtins import _Opener, _P, _PositiveInteger, _SupportsSynchronousAnext, _T_co

_Opener
_P
_PositiveInteger
_SupportsSynchronousAnext
_T_co
```

## Private names in custom builtins

A private runtime value defined by a project-level builtins stub remains available implicitly.
Private generic helpers in the same stub do not.

```py
reveal_type(_private_value)  # revealed: int
reveal_type(_private_type)  # revealed: <class 'int'>

_private_union
_private_typevar

_PrivateTypeVar  # error: [unresolved-reference]
_PrivateParamSpec  # error: [unresolved-reference]
_PrivateTypeVarTuple  # error: [unresolved-reference]
_PrivateAlias  # error: [unresolved-reference]
_PrivateImplicitAlias  # error: [unresolved-reference]
_PrivateListAlias  # error: [unresolved-reference]
_PrivateTupleAlias  # error: [unresolved-reference]
_PrivateCallableAlias  # error: [unresolved-reference]
_PrivateTypeAlias  # error: [unresolved-reference]
```

`__builtins__.pyi`:

```pyi
from types import UnionType
from typing import Callable, ParamSpec, TypeAlias, TypeVar
from typing_extensions import TypeVarTuple

_private_value: int
_private_type = int
_private_union: UnionType
_private_typevar: TypeVar

_PrivateTypeVar = TypeVar("_PrivateTypeVar")
_PrivateParamSpec = ParamSpec("_PrivateParamSpec")
_PrivateTypeVarTuple = TypeVarTuple("_PrivateTypeVarTuple")
_PrivateAlias: TypeAlias = int
_PrivateImplicitAlias = int | str
_PrivateListAlias = list[int]
_PrivateTupleAlias = tuple[int, str]
_PrivateCallableAlias = Callable[[int], str]
_PrivateTypeAlias = type[int]
```

## Conditionally defined private aliases

A private type alias remains unavailable even when its possible definitions come from separate
control-flow branches.

```py
_ConditionalAlias  # error: [unresolved-reference]
```

`__builtins__.pyi`:

```pyi
from typing import TypeAlias

flag: bool

if flag:
    _ConditionalAlias: TypeAlias = int
else:
    _ConditionalAlias: TypeAlias = str
```

## Conditionally defined private protocols

All reachable private protocol definitions must be excluded from implicit builtin lookup.

```py
_ConditionalProtocol  # error: [unresolved-reference]
```

`__builtins__.pyi`:

```pyi
from typing import Protocol, type_check_only

flag: bool

if flag:
    @type_check_only
    class _ConditionalProtocol(Protocol):
        def method(self) -> int: ...

else:
    @type_check_only
    class _ConditionalProtocol(Protocol):
        def method(self) -> str: ...
```

## Re-exported private aliases

Re-exporting a private type alias does not make its original stub-only definition a runtime builtin.

```py
_ImportedAlias  # error: [unresolved-reference]
```

`__builtins__.pyi`:

```pyi
from helpers import _ImportedAlias as _ImportedAlias
```

`helpers.pyi`:

```pyi
from typing import TypeAlias

_ImportedAlias: TypeAlias = int
```

## Conditionally defined private runtime values

A private name remains available when any reachable definition represents a real runtime value.

```py
_ConditionalValue
```

`__builtins__.pyi`:

```pyi
from typing import TypeAlias

flag: bool

if flag:
    _ConditionalValue: TypeAlias = int
else:
    _ConditionalValue: int
```

## Explicitly writing private builtin aliases

Writing to the builtin module is explicit attribute access, so it must not apply implicit builtin
visibility rules.

```py
import builtins

builtins._PrivateAlias = int
```

`__builtins__.pyi`:

```pyi
from typing import TypeAlias

_PrivateAlias: TypeAlias = int
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
