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

## User stub overlay for an installed stub package

User-provided stubs on an extra path can fill gaps in an installed stub package without having to
duplicate the installed package's `__init__.pyi`. The user stub directory is intentionally a
namespace package so that imports not supplied by the overlay still fall back to the installed
stubs.

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

## User stub overlay for a first-party package

Regression test for <https://github.com/astral-sh/ty/issues/3870>.

A user-provided namespace package on an extra path can override a submodule of a regular first-party
package. Modules not provided by the overlay still resolve from the runtime package.

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

## Python source on an extra path is not a stub overlay

An implicit namespace package on an extra path only overlays a regular package when it provides a
stub for the requested module. Ordinary Python source follows runtime namespace-package shadowing.

```toml
[environment]
extra-paths = ["/extra"]
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

`main.py`:

```py
from pkg_source.vendor.get_rate import RATE

reveal_type(RATE)  # revealed: Literal[1]
```

## User stub overlay for an editable package

Regression test for <https://github.com/astral-sh/ty/issues/3870>.

The same overlay behavior applies when the runtime package is available through an editable
installation instead of a first-party root.

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

## User partial stub package for a first-party package

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

### Stub package takes priority over a regular stub

Both candidates contain stubs and have user-controlled extra paths, but the explicit `shapes-stubs`
package is more specific than the regular `shapes` package.

```toml
[environment]
extra-paths = ["/path-one", "/path-two"]
```

`/path-one/shapes/__init__.pyi`:

```pyi
class Pentagon:
    sides: str
```

`/path-two/shapes-stubs/__init__.pyi`:

```pyi
class Pentagon:
    sides: int
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
