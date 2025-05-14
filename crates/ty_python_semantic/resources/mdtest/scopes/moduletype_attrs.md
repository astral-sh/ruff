# Implicit globals from `types.ModuleType`

## Implicit `ModuleType` globals

All modules are instances of `types.ModuleType`. If a name can't be found in any local or global
scope, we look it up as an attribute on `types.ModuleType` in typeshed before deciding that the name
is unbound.

```py
reveal_type(__name__)  # revealed: str
# Typeshed says this is str | None, but for a pure-Python on-disk module its always str
reveal_type(__file__)  # revealed: str
reveal_type(__loader__)  # revealed: LoaderProtocol | None
reveal_type(__package__)  # revealed: str | None
reveal_type(__doc__)  # revealed: str | None
reveal_type(__spec__)  # revealed: ModuleSpec | None
reveal_type(__path__)  # revealed: MutableSequence[str]

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

## `ModuleType` globals combined with explicit assignments and declarations

A `ModuleType` attribute can be overridden in the global scope with a different type, but it must be
a type assignable to the declaration on `ModuleType` unless it is accompanied by an explicit
redeclaration:

`module.py`:

```py
__file__ = None
__path__: list[str] = []
__doc__: int  # error: [invalid-declaration] "Cannot declare type `int` for inferred type `str | None`"
# error: [invalid-declaration] "Cannot shadow implicit global attribute `__package__` with declaration of type `int`"
__package__: int = 42
__spec__ = 42  # error: [invalid-assignment] "Object of type `Literal[42]` is not assignable to `ModuleSpec | None`"
```

`main.py`:

```py
import module

reveal_type(module.__file__)  # revealed: Unknown | None
reveal_type(module.__path__)  # revealed: list[str]
reveal_type(module.__doc__)  # revealed: Unknown
reveal_type(module.__spec__)  # revealed: Unknown | ModuleSpec | None

def nested_scope():
    global __loader__
    reveal_type(__loader__)  # revealed: LoaderProtocol | None
    __loader__ = 56  # error: [invalid-assignment] "Object of type `Literal[56]` is not assignable to `LoaderProtocol | None`"
```

## Accessed as attributes

`ModuleType` attributes can also be accessed as attributes on module-literal types. The special
attributes `__dict__` and `__init__`, and all attributes on `builtins.object`, can also be accessed
as attributes on module-literal types, despite the fact that these are inaccessible as globals from
inside the module:

```py
import typing

reveal_type(typing.__name__)  # revealed: str
reveal_type(typing.__init__)  # revealed: bound method ModuleType.__init__(name: str, doc: str | None = ellipsis) -> None

# For a stub module, we don't know that `__file__` is a string (at runtime it may be entirely
# unset, but we follow typeshed here):
reveal_type(typing.__file__)  # revealed: str | None

# These come from `builtins.object`, not `types.ModuleType`:
reveal_type(typing.__eq__)  # revealed: bound method ModuleType.__eq__(value: object, /) -> bool

reveal_type(typing.__class__)  # revealed: <class 'ModuleType'>

reveal_type(typing.__dict__)  # revealed: dict[str, Any]
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

reveal_type(foo.__dict__)  # revealed: dict[str, Any]
reveal_type(foo_dict)  # revealed: dict[str, Any]
```

## Conditionally global or `ModuleType` attribute

Attributes overridden in the module namespace take priority. If a builtin name is conditionally
defined as a global, however, a name lookup should union the `ModuleType` type with the
conditionally defined type:

```py
__file__ = "foo"

def returns_bool() -> bool:
    return True

if returns_bool():
    __name__ = 1  # error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to `str`"

reveal_type(__file__)  # revealed: Literal["foo"]
reveal_type(__name__)  # revealed: str
```

## Conditionally global or `ModuleType` attribute, with annotation

The same is true if the name is annotated:

```py
# error: [invalid-declaration] "Cannot shadow implicit global attribute `__file__` with declaration of type `int`"
__file__: int = 42

def returns_bool() -> bool:
    return True

if returns_bool():
    # error: [invalid-declaration] "Cannot shadow implicit global attribute `__name__` with declaration of type `int`"
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
