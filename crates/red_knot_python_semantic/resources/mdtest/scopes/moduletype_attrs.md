# Implicit globals from `types.ModuleType`

## Implicit `ModuleType` globals

All modules are instances of `types.ModuleType`. If a name can't be found in any local or global
scope, we look it up as an attribute on `types.ModuleType` in typeshed before deciding that the name
is unbound.

```py
reveal_type(__name__)  # revealed: str
reveal_type(__file__)  # revealed: str | None
reveal_type(__loader__)  # revealed: LoaderProtocol | None
reveal_type(__package__)  # revealed: str | None
reveal_type(__doc__)  # revealed: str | None
reveal_type(__spec__)  # revealed: ModuleSpec | None

reveal_type(__path__)  # revealed: @Todo(generics)

class X:
    reveal_type(__name__)  # revealed: str

def foo():
    reveal_type(__name__)  # revealed: str
```

However, three attributes on `types.ModuleType` are not present as implicit module globals; these
are excluded:

```py
# error: [unresolved-reference]
# revealed: Unknown
reveal_type(__getattr__)

# error: [unresolved-reference]
# revealed: Unknown
reveal_type(__dict__)

# error: [unresolved-reference]
# revealed: Unknown
reveal_type(__init__)
```

## Accessed as attributes

`ModuleType` attributes can also be accessed as attributes on module-literal types. The special
attributes `__dict__` and `__init__`, and all attributes on `builtins.object`, can also be accessed
as attributes on module-literal types, despite the fact that these are inaccessible as globals from
inside the module:

```py
import typing

reveal_type(typing.__name__)  # revealed: str
reveal_type(typing.__init__)  # revealed: <bound method `__init__` of `ModuleType`>

# These come from `builtins.object`, not `types.ModuleType`:
reveal_type(typing.__eq__)  # revealed: <bound method `__eq__` of `ModuleType`>

reveal_type(typing.__class__)  # revealed: Literal[ModuleType]

# TODO: needs support generics; should be `dict[str, Any]`:
reveal_type(typing.__dict__)  # revealed: @Todo(generics)
```

Typeshed includes a fake `__getattr__` method in the stub for `types.ModuleType` to help out with
dynamic imports; but we ignore that for module-literal types where we know exactly which module
we're dealing with:

```py
# error: [unresolved-attribute]
reveal_type(typing.__getattr__)  # revealed: Unknown
```

## `types.ModuleType.__dict__` takes precedence over global variable `__dict__`

It's impossible to override the `__dict__` attribute of `types.ModuleType` instances from inside the
module; we should prioritise the attribute in the `types.ModuleType` stub over a variable named
`__dict__` in the module's global namespace:

`foo.py`:

```py
__dict__ = "foo"

reveal_type(__dict__)  # revealed: Literal["foo"]
```

`bar.py`:

```py
import foo
from foo import __dict__ as foo_dict

# TODO: needs support generics; should be `dict[str, Any]` for both of these:
reveal_type(foo.__dict__)  # revealed: @Todo(generics)
reveal_type(foo_dict)  # revealed: @Todo(generics)
```

## Conditionally global or `ModuleType` attribute

Attributes overridden in the module namespace take priority. If a builtin name is conditionally
defined as a global, however, a name lookup should union the `ModuleType` type with the
conditionally defined type:

```py
__file__ = 42

def returns_bool() -> bool:
    return True

if returns_bool():
    __name__ = 1

reveal_type(__file__)  # revealed: Literal[42]
reveal_type(__name__)  # revealed: Literal[1] | str
```

## Conditionally global or `ModuleType` attribute, with annotation

The same is true if the name is annotated:

```py
__file__: int = 42

def returns_bool() -> bool:
    return True

if returns_bool():
    __name__: int = 1

reveal_type(__file__)  # revealed: Literal[42]
reveal_type(__name__)  # revealed: Literal[1] | str
```

## Implicit global attributes in the current module override implicit globals from builtins

Here, we take the type of the implicit global symbol `__name__` from the `types.ModuleType` stub
(which in this custom typeshed specifies the type as `bytes`). This is because the `main` module has
an implicit `__name__` global that shadows the builtin `__name__` symbol.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class int: ...
class bytes: ...

__name__: int = 42
```

`/typeshed/stdlib/types.pyi`:

```pyi
class ModuleType:
    __name__: bytes
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

`main.py`:

```py
reveal_type(__name__)  # revealed: bytes
```
