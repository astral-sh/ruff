# Unused dependencies

ty reports a dependency declared in `pyproject.toml` when no checked source file imports a module
it provides. For example, the `pillow` distribution provides the `PIL` package.

```toml
[rules]
unused-dependency = "warn"

[packages]
pillow = ["PIL"]
```

## Declaring a dependency without importing it

`pyproject.toml`:

```toml
[project] # snapshot: unused-dependency
dependencies = ["pillow"]
```

```snapshot
warning[unused-dependency]: Dependency `pillow` is declared but never imported
 --> project/pyproject.toml:1:1
  |
1 | [project] # snapshot: unused-dependency
  | ^
```

`main.py`:

```py
print("Hello!")
```

## Importing a declared dependency

`pyproject.toml`:

```toml
[project]
dependencies = ["pillow"]
```

`main.py`:

```py
import PIL  # no diagnostic
```

## Selecting which files to check

The source filters select only `main.py`. An import in `generated.py` doesn't count, even though
`main.py` imports that file. An import in a file outside `src.include` doesn't count either.

`ty.toml`:

```toml
[src]
include = ["main.py", "generated.py"]
exclude = ["generated.py"]
```

`pyproject.toml`:

```toml
[project] # error: [unused-dependency]
dependencies = ["pillow"]
```

`main.py`:

```py
import generated  # no diagnostic
```

`generated.py`:

```py
import PIL
```

`other.py`:

```py
import PIL
```

## Edge cases

### A local package shadows an installed package

The installed `images` package provides `images.filters`. Python finds the local `images` package
first, so neither import uses the installed distribution.

```toml
[rules]
unused-dependency = "warn"

[packages]
images = ["images", "images.filters"]
```

`pyproject.toml`:

```toml
[project] # error: [unused-dependency]
dependencies = ["images"]
```

`images/__init__.py`:

```py
```

`main.py`:

```py
import images  # no diagnostic
import images.filters  # error: [unresolved-import]
```

### Type stubs for an installed package

ty reads `requests-stubs` to type check the import, but Python loads `requests` at runtime. The
rule counts the import for `requests` and skips the stub distribution, which provides no runtime
module of its own.

```toml
[rules]
unused-dependency = "warn"

[packages]
requests = ["requests"]
requests-stubs = []
```

`pyproject.toml`:

```toml
[project]
dependencies = ["requests", "requests-stubs"]
```

`.venv/<path-to-site-packages>/requests-stubs/__init__.pyi`:

```pyi
```

`main.py`:

```py
import requests  # no diagnostic
```

### Namespace packages

Both `a-lib` and `b-lib` provide modules in the `shared` namespace.

```toml
[rules]
unused-dependency = "warn"

[packages]
a-lib = ["shared", "shared.a"]
b-lib = ["shared", "shared.b"]
```

#### Importing the namespace uses both distributions

`pyproject.toml`:

```toml
[project]
dependencies = ["a-lib", "b-lib"]
```

`main.py`:

```py
import shared  # no diagnostic
```

#### Importing a child uses only its owner

`pyproject.toml`:

```toml
[project] # error: [unused-dependency] "`b-lib`"
dependencies = ["a-lib", "b-lib"]
```

`main.py`:

```py
from shared import a  # no diagnostic
```

### Importing a child runs its parent's initializer

When one distribution provides `shared/__init__.py` and another provides `shared.child`, importing
the child uses both distributions.

```toml
[rules]
unused-dependency = "warn"

[packages]
parent-lib = ["shared"]
child-lib = ["shared.child"]
```

`pyproject.toml`:

```toml
[project]
dependencies = ["parent-lib", "child-lib"]
```

`.venv/<path-to-site-packages>/shared/__init__.py`:

```py
```

`main.py`:

```py
import shared.child  # no diagnostic
```
