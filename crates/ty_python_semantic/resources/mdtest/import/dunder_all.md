# `__all__`

Reference:
<https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols>

NOTE: This file only includes the usage of `__all__` for named-imports i.e.,
`from module import symbol`. For the usage of `__all__` in wildcard imports, refer to
[star.md](star.md).

## Undefined

`exporter.py`:

```py
class A: ...
class B: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: None
reveal_type(dunder_all_names(exporter))
```

## Global scope

The `__all__` variable is only recognized from the global scope of the module. It is not recognized
from the local scope of a function or class.

`exporter.py`:

```py
__all__ = ["A"]

def foo():
    __all__.append("B")

class Foo:
    __all__ += ["C"]

class A: ...
class B: ...
class C: ...

foo()
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"]]
reveal_type(dunder_all_names(exporter))
```

## Supported idioms

According to the [specification], the following idioms are supported:

### List assignment

`exporter.py`:

```py
__all__ = ["A", "B"]

class A: ...
class B: ...
```

`exporter_annotated.py`:

```py
__all__: list[str] = ["C", "D"]

class C: ...
class D: ...
```

`importer.py`:

```py
import exporter
import exporter_annotated
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(dunder_all_names(exporter))

# revealed: tuple[Literal["C"], Literal["D"]]
reveal_type(dunder_all_names(exporter_annotated))
```

### List assignment (shadowed)

`exporter.py`:

```py
__all__ = ["A", "B"]

class A: ...
class B: ...

__all__ = ["C", "D"]

class C: ...
class D: ...
```

`exporter_annotated.py`:

```py
__all__ = ["X"]

class X: ...

__all__: list[str] = ["Y", "Z"]

class Y: ...
class Z: ...
```

`importer.py`:

```py
import exporter
import exporter_annotated
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["C"], Literal["D"]]
reveal_type(dunder_all_names(exporter))

# revealed: tuple[Literal["Y"], Literal["Z"]]
reveal_type(dunder_all_names(exporter_annotated))
```

### Tuple assignment

`exporter.py`:

```py
__all__ = ("A", "B")

class A: ...
class B: ...
```

`exporter_annotated.py`:

```py
__all__: tuple[str, ...] = ("C", "D")

class C: ...
class D: ...
```

`importer.py`:

```py
import exporter
import exporter_annotated
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(dunder_all_names(exporter))

# revealed: tuple[Literal["C"], Literal["D"]]
reveal_type(dunder_all_names(exporter_annotated))
```

### Tuple assignment (shadowed)

`exporter.py`:

```py
__all__ = ("A", "B")

class A: ...
class B: ...

__all__ = ("C", "D")

class C: ...
class D: ...
```

`exporter_annotated.py`:

```py
__all__ = ("X",)

class X: ...

__all__: tuple[str, ...] = ("Y", "Z")

class Y: ...
class Z: ...
```

`importer.py`:

```py
import exporter
import exporter_annotated
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["C"], Literal["D"]]
reveal_type(dunder_all_names(exporter))

# revealed: tuple[Literal["Y"], Literal["Z"]]
reveal_type(dunder_all_names(exporter_annotated))
```

### Augmenting list with a list or submodule `__all__`

`subexporter.py`:

```py
__all__ = ["A", "B"]

class A: ...
class B: ...
```

`exporter.py`:

```py
import subexporter

__all__ = []
__all__ += ["C", "D"]
__all__ += subexporter.__all__

class C: ...
class D: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["B"], Literal["C"], Literal["D"]]
reveal_type(dunder_all_names(exporter))
```

### Augmenting list with a list or submodule `__all__` (2)

The same again, but the submodule is an attribute expression rather than a name expression:

`exporter/__init__.py`:

```py
```

`exporter/sub.py`:

```py
__all__ = ["foo"]

foo = 42
```

`exporter/sub2.py`:

```py
__all__ = ["bar"]

bar = 56
```

`module.py`:

```py
import exporter.sub
import exporter.sub2

__all__ = []

if True:
    __all__.extend(exporter.sub.__all__)
    __all__ += exporter.sub2.__all__
```

`main.py`:

```py
import module
from ty_extensions import dunder_all_names

reveal_type(dunder_all_names(module))  # revealed: tuple[Literal["bar"], Literal["foo"]]
```

### Extending with a list or submodule `__all__`

`subexporter.py`:

```py
__all__ = ["A", "B"]

class A: ...
class B: ...
```

`exporter.py`:

```py
import subexporter

__all__ = []
__all__.extend(["C", "D"])
__all__.extend(("E", "F"))
__all__.extend({"G", "H"})
__all__.extend(subexporter.__all__)

class C: ...
class D: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["B"], Literal["C"], Literal["D"], Literal["E"], Literal["F"], Literal["G"], Literal["H"]]
reveal_type(dunder_all_names(exporter))
```

### Appending a single symbol

`exporter.py`:

```py
__all__ = ["A"]
__all__.append("B")

class A: ...
class B: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(dunder_all_names(exporter))
```

### Removing a single symbol

`exporter.py`:

```py
__all__ = ["A", "B"]
__all__.remove("A")

# Non-existant symbol in `__all__` at this point
# TODO: This raises `ValueError` at runtime, maybe we should raise a diagnostic as well?
__all__.remove("C")

class A: ...
class B: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["B"]]
reveal_type(dunder_all_names(exporter))
```

### Mixed

`subexporter.py`:

```py
__all__ = []

__all__ = ["A"]
__all__.append("B")
__all__.extend(["C"])
__all__.remove("B")

class A: ...
class B: ...
class C: ...
```

`exporter.py`:

```py
import subexporter

__all__ = []
__all__ += ["D"]
__all__ += subexporter.__all__

class D: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["C"], Literal["D"]]
reveal_type(dunder_all_names(exporter))
```

## Invalid

### Unsupported idioms

Idioms that are not mentioned in the [specification] are not recognized by `ty` and if they're used,
`__all__` is considered to be undefined for that module. This is to avoid false positives.

`bar.py`:

```py
__all__ = ["A", "B"]

class A: ...
class B: ...
```

`foo.py`:

```py
import bar as bar
```

`exporter.py`:

```py
import foo
from ty_extensions import dunder_all_names

__all__ = []

# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(dunder_all_names(foo.bar))

# Only direct attribute access of modules are recognized
# TODO: warning diagnostic
__all__.extend(foo.bar.__all__)
# TODO: warning diagnostic
__all__ += foo.bar.__all__

# Augmented assignment is only allowed when the value is a list expression
# TODO: warning diagnostic
__all__ += ("C",)

# Other methods on `list` are not recognized
# TODO: warning diagnostic
__all__.insert(0, "C")
# TODO: warning diagnostic
__all__.clear()

__all__.append("C")
# `pop` is not valid; use `remove` instead
# TODO: warning diagnostic
__all__.pop()

# Sets are not recognized
# TODO: warning diagnostic
__all__ = {"C", "D"}

class C: ...
class D: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: None
reveal_type(dunder_all_names(exporter))
```

### Non-string elements

Similarly, if `__all__` contains any non-string elements, we will consider `__all__` to not be
defined for that module. This is also to avoid false positives.

`subexporter.py`:

```py
__all__ = ("A", "B")

class A: ...
class B: ...
```

`exporter1.py`:

```py
import subexporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(dunder_all_names(subexporter))

# TODO: warning diagnostic
__all__ = ("C", *subexporter.__all__)

class C: ...
```

`importer.py`:

```py
import exporter1
from ty_extensions import dunder_all_names

# revealed: None
reveal_type(dunder_all_names(exporter1))
```

## Statically known branches

### Python 3.10

```toml
[environment]
python-version = "3.10"
```

`exporter.py`:

```py
import sys

__all__ = ["AllVersion"]

if sys.version_info >= (3, 12):
    __all__ += ["Python312"]
elif sys.version_info >= (3, 11):
    __all__ += ["Python311"]
else:
    __all__ += ["Python310"]

class AllVersion: ...

if sys.version_info >= (3, 12):
    class Python312: ...

elif sys.version_info >= (3, 11):
    class Python311: ...

else:
    class Python310: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["AllVersion"], Literal["Python310"]]
reveal_type(dunder_all_names(exporter))
```

### Python 3.11

```toml
[environment]
python-version = "3.11"
```

`exporter.py`:

```py
import sys

__all__ = ["AllVersion"]

if sys.version_info >= (3, 12):
    __all__ += ["Python312"]
elif sys.version_info >= (3, 11):
    __all__ += ["Python311"]
else:
    __all__ += ["Python310"]

class AllVersion: ...

if sys.version_info >= (3, 12):
    class Python312: ...

elif sys.version_info >= (3, 11):
    class Python311: ...

else:
    class Python310: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["AllVersion"], Literal["Python311"]]
reveal_type(dunder_all_names(exporter))
```

### Python 3.12

```toml
[environment]
python-version = "3.12"
```

`exporter.py`:

```py
import sys

__all__ = ["AllVersion"]

if sys.version_info >= (3, 12):
    __all__ += ["Python312"]
elif sys.version_info >= (3, 11):
    __all__ += ["Python311"]
else:
    __all__ += ["Python310"]

class AllVersion: ...

if sys.version_info >= (3, 12):
    class Python312: ...

elif sys.version_info >= (3, 11):
    class Python311: ...

else:
    class Python310: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["AllVersion"], Literal["Python312"]]
reveal_type(dunder_all_names(exporter))
```

### Multiple `if` statements

```toml
[environment]
python-version = "3.11"
```

`exporter.py`:

```py
import sys

__all__ = ["AllVersion"]

if sys.version_info >= (3, 12):
    __all__ += ["Python312"]

if sys.version_info >= (3, 11):
    __all__ += ["Python311"]

if sys.version_info >= (3, 10):
    __all__ += ["Python310"]

class AllVersion: ...

if sys.version_info >= (3, 12):
    class Python312: ...

if sys.version_info >= (3, 11):
    class Python311: ...

if sys.version_info >= (3, 10):
    class Python310: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["AllVersion"], Literal["Python310"], Literal["Python311"]]
reveal_type(dunder_all_names(exporter))
```

## Origin

`__all__` can be defined in a module mainly in the following three ways:

### Directly in the module

`exporter.py`:

```py
__all__ = ["A"]

class A: ...
```

`importer.py`:

```py
import exporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"]]
reveal_type(dunder_all_names(exporter))
```

### Using named import

`subexporter.py`:

```py
__all__ = ["A"]

class A: ...
```

`exporter.py`:

```py
from subexporter import __all__

__all__.append("B")

class B: ...
```

`importer.py`:

```py
import exporter
import subexporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"]]
reveal_type(dunder_all_names(subexporter))
# revealed: tuple[Literal["A"], Literal["B"]]
reveal_type(dunder_all_names(exporter))
```

### Using wildcard import (1)

Wildcard import doesn't export `__all__` unless it is explicitly included in the `__all__` of the
module.

`subexporter.py`:

```py
__all__ = ["A", "__all__"]

class A: ...
```

`exporter.py`:

```py
from subexporter import *

# TODO: Should be `list[str]`
# TODO: Should we avoid including `Unknown` for this case?
reveal_type(__all__)  # revealed: Unknown | list[Unknown]

__all__.append("B")

class B: ...
```

`importer.py`:

```py
import exporter
import subexporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"], Literal["__all__"]]
reveal_type(dunder_all_names(subexporter))
# revealed: tuple[Literal["A"], Literal["B"], Literal["__all__"]]
reveal_type(dunder_all_names(exporter))
```

### Using wildcard import (2)

`subexporter.py`:

```py
__all__ = ["A"]

class A: ...
```

`exporter.py`:

```py
from subexporter import *

# error: [unresolved-reference]
reveal_type(__all__)  # revealed: Unknown

# error: [unresolved-reference]
__all__.append("B")

class B: ...
```

`importer.py`:

```py
import exporter
import subexporter
from ty_extensions import dunder_all_names

# revealed: tuple[Literal["A"]]
reveal_type(dunder_all_names(subexporter))
# revealed: None
reveal_type(dunder_all_names(exporter))
```

[specification]: https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols
