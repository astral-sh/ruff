# Stub packages

Stub packages are packages named `<package>-stubs` that provide typing stubs for `<package>`. See
[specification](https://typing.python.org/en/latest/spec/distributing.html#stub-only-packages).

## Simple stub

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
class Foo:
    name: str
    age: int
```

`/packages/foo/__init__.py`:

```py
class Foo: ...
```

`main.py`:

```py
from foo import Foo

reveal_type(Foo().name)  # revealed: str
```

## Stubs only

The regular package isn't required for type checking.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
class Foo:
    name: str
    age: int
```

`main.py`:

```py
from foo import Foo

reveal_type(Foo().name)  # revealed: str
```

## `-stubs` named module

A module named `<module>-stubs` isn't a stub package.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs.pyi`:

```pyi
class Foo:
    name: str
    age: int
```

`main.py`:

```py
from foo import Foo  # error: [unresolved-import]

reveal_type(Foo().name)  # revealed: Unknown
```

## Namespace package in different search paths

A namespace package with multiple stub packages spread over multiple search paths.

```toml
[environment]
extra-paths = ["/stubs1", "/stubs2", "/packages"]
```

`/stubs1/shapes-stubs/polygons/pentagon.pyi`:

```pyi
class Pentagon:
    sides: int
    area: float
```

`/stubs2/shapes-stubs/polygons/hexagon.pyi`:

```pyi
class Hexagon:
    sides: int
    area: float
```

`/packages/shapes/polygons/pentagon.py`:

```py
class Pentagon: ...
```

`/packages/shapes/polygons/hexagon.py`:

```py
class Hexagon: ...
```

`main.py`:

```py
from shapes.polygons.hexagon import Hexagon
from shapes.polygons.pentagon import Pentagon

reveal_type(Pentagon().sides)  # revealed: int
reveal_type(Hexagon().area)  # revealed: int | float
```

## Manual overrides from extra paths

The typing specification defines the first tier of the
[import resolution order](https://typing.python.org/en/latest/spec/distributing.html#import-resolution-ordering)
as follows:

> Stubs or Python source manually put in the beginning of the path. Type checkers SHOULD provide
> this to allow the user complete control.

Extra paths provide ty's highest-priority user-controlled search tier. Their directory names have no
special meaning, and ordinary search-path and namespace-package shadowing rules apply within the
tier. During typing resolution, a stub found through an extra path can supplement or override a
package from a lower-priority root without duplicating its parent packages.

### Installed stub package

A stub on an extra path can fill a gap in an installed stub package without duplicating the
installed package's `__init__.pyi`. Imports not supplied by the override still fall back to the
installed stubs.

```toml
[environment]
python = "/.venv"
extra-paths = ["/stubs"]
```

`/stubs/PyInstaller/archive/readers.pyi`:

```pyi
class Reader: ...
```

`/.venv/<path-to-site-packages>/PyInstaller-stubs/__init__.pyi`:

```pyi
class Analysis: ...
```

`/.venv/<path-to-site-packages>/PyInstaller-stubs/py.typed`:

```text
```

`main.py`:

```py
from PyInstaller import Analysis
from PyInstaller.archive.readers import Reader

reveal_type(Reader)  # revealed: <class 'Reader'>
reveal_type(Analysis)  # revealed: <class 'Analysis'>
```

### First-party package

Regression test for <https://github.com/astral-sh/ty/issues/3870>.

A stub on an extra path can override a submodule of a lower-priority first-party package. Modules
not provided by the override still resolve from the runtime package.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/pkg_a/vendor/get_rate.pyi`:

```pyi
RATE: str
```

`/src/pkg_a/__init__.py`:

```py
ROOT = "runtime"
```

`/src/pkg_a/vendor/__init__.py`:

```py
```

`/src/pkg_a/vendor/get_rate.py`:

```py
RATE = 1
```

`main.py`:

```py
from pkg_a import ROOT
from pkg_a.vendor.get_rate import RATE

reveal_type(ROOT)  # revealed: Literal["runtime"]
reveal_type(RATE)  # revealed: str
```

### Editable package

Regression test for <https://github.com/astral-sh/ty/issues/3870>.

A stub on an extra path can override a submodule of a lower-priority editable package. Modules not
provided by the override still resolve from the editable package.

```toml
[environment]
python = "/.venv"
extra-paths = ["/stubs"]
```

`/stubs/pkg_b/vendor/get_rate.pyi`:

```pyi
RATE: str
```

`/.venv/<path-to-site-packages>/pkg-b.pth`:

```pth
/editable
```

`/editable/pkg_b/__init__.py`:

```py
ROOT = "runtime"
```

`/editable/pkg_b/vendor/__init__.py`:

```py
```

`/editable/pkg_b/vendor/get_rate.py`:

```py
RATE = 1
```

`main.py`:

```py
from pkg_b import ROOT
from pkg_b.vendor.get_rate import RATE

reveal_type(ROOT)  # revealed: Literal["runtime"]
reveal_type(RATE)  # revealed: str
```

### Stub and runtime package shapes may differ

An extra-path stub takes precedence over lower-priority runtime modules. The resolver does not
validate that the runtime package has the same shape as the stub tree.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/sys/child.pyi`:

```pyi
VALUE: str
```

`/stubs/modroot/child.pyi`:

```pyi
VALUE: str
```

`/stubs/pkg/mid/child.pyi`:

```pyi
VALUE: str
```

`/src/modroot.py`:

```py
VALUE = 1
```

`/src/pkg/__init__.py`:

```py
```

`/src/pkg/mid.py`:

```py
VALUE = 1
```

`main.py`:

```py
from modroot.child import VALUE as root_child
from pkg.mid.child import VALUE as intermediate_child
from sys.child import VALUE as builtin_child

reveal_type(builtin_child)  # revealed: str
reveal_type(root_child)  # revealed: str
reveal_type(intermediate_child)  # revealed: str
```

### Ordinary ordering still applies within the extra-path tier

Extra paths are not independent stub roots. A later regular package shadows an earlier namespace
package, even when that namespace contains a stub for the requested module. A `.py` file inside an
extra-path namespace follows normal namespace-package resolution and does not override a regular
package from a lower-priority root.

```toml
[environment]
extra-paths = ["/extra", "/runtime"]
```

`/extra/pkg_source/vendor/get_rate.py`:

```py
RATE = "extra"
```

`/src/pkg_source/__init__.py`:

```py
```

`/src/pkg_source/vendor/__init__.py`:

```py
```

`/src/pkg_source/vendor/get_rate.py`:

```py
RATE = 1
```

`/extra/pkg_stub/child.pyi`:

```pyi
VALUE: str
```

`/runtime/pkg_stub/__init__.py`:

```py
```

`/runtime/pkg_stub/child.py`:

```py
VALUE = 1
```

`main.py`:

```py
from pkg_source.vendor.get_rate import RATE
from pkg_stub.child import VALUE

reveal_type(RATE)  # revealed: Literal[1]
reveal_type(VALUE)  # revealed: Literal[1]
```

## PEP 561 partial stub package for a first-party package

Regression test for <https://github.com/astral-sh/ty/issues/3770>.

A partial stub package on an extra path overrides the modules it provides and falls back to the
regular first-party package for missing modules and package attributes.

```toml
[environment]
extra-paths = ["/typings"]
```

`/typings/lib-stubs/py.typed`:

```text
partial
```

`/typings/lib-stubs/b.pyi`:

```pyi
c: int
```

`/src/lib/__init__.py`:

```py
a = "runtime"
```

`/src/lib/b.py`:

```py
c = "runtime"
```

`/src/lib/runtime_only.py`:

```py
value = 42
```

`main.py`:

```py
from lib import a
from lib.b import c
from lib.runtime_only import value

reveal_type(a)  # revealed: Literal["runtime"]
reveal_type(c)  # revealed: int
reveal_type(value)  # revealed: Literal[42]
```

## PEP 561 partial stub package before a runtime extra path

Unlike a loose namespace stub tree, a PEP 561 partial stub package keeps stub-package precedence
when a regular runtime package is available on a later extra path.

```toml
[environment]
extra-paths = ["/stubs", "/runtime"]
```

`/stubs/pkg-stubs/py.typed`:

```text
partial
```

`/stubs/pkg-stubs/child.pyi`:

```pyi
VALUE: str
```

`/runtime/pkg/__init__.py`:

```py
```

`/runtime/pkg/child.py`:

```py
VALUE = 1
```

`main.py`:

```py
from pkg.child import VALUE

reveal_type(VALUE)  # revealed: str
```

## Inconsistent stub packages

Stub packages where one is a namespace package and the other is a regular package. Module resolution
should stop after the first non-namespace stub package. This matches Pyright's behavior.

```toml
[environment]
extra-paths = ["/stubs1", "/stubs2", "/packages"]
```

`/stubs1/shapes-stubs/__init__.pyi`:

```pyi
```

`/stubs1/shapes-stubs/polygons/__init__.pyi`:

```pyi
```

`/stubs1/shapes-stubs/polygons/pentagon.pyi`:

```pyi
class Pentagon:
    sides: int
    area: float
```

`/stubs2/shapes-stubs/polygons/hexagon.pyi`:

```pyi
class Hexagon:
    sides: int
    area: float
```

`/packages/shapes/polygons/pentagon.py`:

```py
class Pentagon: ...
```

`/packages/shapes/polygons/hexagon.py`:

```py
class Hexagon: ...
```

`main.py`:

```py
from shapes.polygons.pentagon import Pentagon
from shapes.polygons.hexagon import Hexagon  # error: [unresolved-import]

reveal_type(Pentagon().sides)  # revealed: int
reveal_type(Hexagon().area)  # revealed: Unknown
```

## Namespace stubs for non-namespace package

The runtime package is a regular package but the stubs are namespace packages. Pyright skips the
stub package if the "regular" package isn't a namespace package. I'm not aware that the behavior
here is specified, but we currently agree with pyright here.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/shapes-stubs/polygons/pentagon.pyi`:

```pyi
class Pentagon: ...
```

`/packages/shapes-stubs/polygons/hexagon.pyi`:

```pyi
class Hexagon: ...
```

`/packages/shapes/__init__.py`:

```py
```

`/packages/shapes/polygons/__init__.py`:

```py
```

`/packages/shapes/polygons/pentagon.py`:

```py
class Pentagon:
    sides: int
    area: float
```

`/packages/shapes/polygons/hexagon.py`:

```py
class Hexagon:
    sides: int
    area: float
```

`main.py`:

```py
from shapes.polygons.pentagon import Pentagon
from shapes.polygons.hexagon import Hexagon

reveal_type(Pentagon().sides)  # revealed: int
reveal_type(Hexagon().area)  # revealed: int | float
```

## Stub package using `__init__.py` over `.pyi`

It's recommended that stub packages use `__init__.pyi` files over `__init__.py` but it doesn't seem
to be an enforced convention. At least, Pyright is fine with the following.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/shapes-stubs/__init__.py`:

```py
class Pentagon:
    sides: int
    area: float

class Hexagon:
    sides: int
    area: float
```

`/packages/shapes/__init__.py`:

```py
class Pentagon: ...
class Hexagon: ...
```

`main.py`:

```py
from shapes import Hexagon, Pentagon

reveal_type(Pentagon().sides)  # revealed: int
reveal_type(Hexagon().area)  # revealed: int | float
```

## Relative import in stub package

Regression test for <https://github.com/astral-sh/ty/issues/408>

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/yaml-stubs/__init__.pyi`:

```pyi
from .loader import *
```

`/packages/yaml-stubs/loader.pyi`:

```pyi
class YamlLoader: ...
```

`main.py`:

```py
import yaml

reveal_type(yaml.YamlLoader)  # revealed: <class 'YamlLoader'>
```

## Priority across search paths

Within the user-controlled extra-path tier, ty gives a `foo-stubs` stub package priority over a
regular `foo` package regardless of search-path ordering. Search-path order breaks ties between
candidates of the same kind. This matches Pyright's behavior and is required for
<https://github.com/astral-sh/ty/issues/1967>.

The typing specification defines the broader [import resolution ordering], but does not specify
ordering between these two kinds of candidates within the user-controlled tier.

### Stub package comes first on the search path

```toml
[environment]
extra-paths = ["/path-one", "/path-two"]
```

`/path-one/shapes-stubs/__init__.pyi`:

```pyi
class Pentagon:
    sides: int
```

`/path-two/shapes/__init__.py`:

```py
class Pentagon: ...
```

`main.py`:

```py
from shapes import Pentagon

reveal_type(Pentagon().sides)  # revealed: int
```

### Stub package comes last on the search path

```toml
[environment]
extra-paths = ["/path-two", "/path-one"]
```

`/path-one/shapes-stubs/__init__.pyi`:

```pyi
class Pentagon:
    sides: int
```

`/path-two/shapes/__init__.py`:

```py
class Pentagon: ...
```

`main.py`:

```py
from shapes import Pentagon

reveal_type(Pentagon().sides)  # revealed: int
```

### Partial stub packages

Because `shapes/bar.pyi` is a stub file, it must take priority over `shapes/foo.py` in the first
search path even though `shapes/bar.pyi` appears in the second search path. But because
`shapes/bar.pyi` is a `partial = true` namespace package, when we fail to find the `foo` submodule
in `/path-two/shapes`, we must fallback to `shapes/foo.py` when resolving the module.

This test exists at the intersection of namespace packages and partial stub packages.

```toml
[environment]
extra-paths = ["/path-one", "/path-two"]
```

`/path-one/shapes/foo.py`:

```py
X = 42
```

`/path-two/shapes/bar.pyi`:

```pyi
```

`/path-two/shapes/py.typed`:

```text
partial = true
```

`main.py`:

```py
from shapes.foo import X

reveal_type(X)  # revealed: Literal[42]
```

[import resolution ordering]: https://typing.python.org/en/latest/spec/distributing.html#import-resolution-ordering
