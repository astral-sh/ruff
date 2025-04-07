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
here is specified, and using the stubs without probing the runtime package first requires slightly
fewer lookups.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/shapes-stubs/polygons/pentagon.pyi`:

```pyi
class Pentagon:
    sides: int
    area: float
```

`/packages/shapes-stubs/polygons/hexagon.pyi`:

```pyi
class Hexagon:
    sides: int
    area: float
```

`/packages/shapes/__init__.py`:

```py
```

`/packages/shapes/polygons/__init__.py`:

```py
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
