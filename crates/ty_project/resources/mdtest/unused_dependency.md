# Unused dependencies

## Used and unused dependencies

This project declares `requests` and `flask`, but only imports `requests`. ty reports `flask` as
unused.

```toml
[rules]
unused-dependency = "warn"

[packages]
requests = ["requests"]
flask = ["flask"]
```

`pyproject.toml`:

```toml
[project] # snapshot: unused-dependency
dependencies = ["requests", "flask"]
```

```snapshot
warning[unused-dependency]: Dependency `flask` is declared but never imported
 --> project/pyproject.toml:1:1
  |
1 | [project] # snapshot: unused-dependency
  | ^
```

`main.py`:

```py
import requests  # no diagnostic
```

`/venv/<path-to-site-packages>/requests.py`:

```py
```

`/venv/<path-to-site-packages>/flask.py`:

```py
```

## Dependency and import names can differ

Importing `PIL` counts as using the `pillow` dependency.

```toml
[rules]
unused-dependency = "warn"

[packages]
pillow = ["PIL"]
```

`pyproject.toml`:

```toml
[project]
dependencies = ["pillow"]
```

`main.py`:

```py
import PIL  # no diagnostic
```

`/venv/<path-to-site-packages>/PIL/__init__.py`:

```py
```

## Source filters

ty counts imports in the files selected by `src.include` and `src.exclude`. Here, only
`src/main.py` is selected. The imports of `flask` in the other files do not count, even though
`main.py` imports `helper`.

```toml
[rules]
unused-dependency = "warn"

[src]
include = ["src"]
exclude = ["src/generated.py"]

[packages]
requests = ["requests"]
flask = ["flask"]
```

`pyproject.toml`:

```toml
[project] # error: [unused-dependency] "Dependency `flask` is declared but never imported"
dependencies = ["requests", "flask"]
```

`src/main.py`:

```py
import requests  # no diagnostic
import helper  # no diagnostic
```

`helper.py`:

```py
import flask  # no diagnostic
```

`src/generated.py`:

```py
import flask  # no diagnostic
```

`/venv/<path-to-site-packages>/requests.py`:

```py
```

`/venv/<path-to-site-packages>/flask.py`:

```py
```

## Edge cases

### Runtime packages and stubs

`shared-lib-stubs` provides type information for `shared_lib`, and `shared-lib` provides its runtime
implementation. Importing `shared_lib` counts as using `shared-lib`, even when ty resolves the
import to the stub. The stub package has no known runtime modules, so the rule skips it.

```toml
[rules]
unused-dependency = "warn"

[packages]
shared-lib = ["shared_lib"]
shared-lib-stubs = []
```

#### Importing the installed package

`pyproject.toml`:

```toml
[project]
dependencies = ["shared-lib", "shared-lib-stubs"]
```

`/venv/<path-to-site-packages>/shared_lib.py`:

```py
```

`/venv/<path-to-site-packages>/shared_lib-stubs/__init__.pyi`:

```pyi
```

`main.py`:

```py
import shared_lib  # no diagnostic
```

#### A local module shadows the installed package

Importing a local module with the same name does not use the installed distribution.

`pyproject.toml`:

```toml
[project] # snapshot: unused-dependency
dependencies = ["shared-lib", "shared-lib-stubs"]
```

```snapshot
warning[unused-dependency]: Dependency `shared-lib` is declared but never imported
 --> project/pyproject.toml:1:1
  |
1 | [project] # snapshot: unused-dependency
  | ^
```

`/venv/<path-to-site-packages>/shared_lib.py`:

```py
```

`/venv/<path-to-site-packages>/shared_lib-stubs/__init__.pyi`:

```pyi
```

`shared_lib.py`:

```py
```

`main.py`:

```py
import shared_lib  # no diagnostic
```

### Namespace packages

Both `a-lib` and `b-lib` contribute modules to the `shared` namespace package.

```toml
[rules]
unused-dependency = "warn"

[packages]
a-lib = ["shared", "shared.a"]
b-lib = ["shared", "shared.b"]
```

#### Importing the namespace counts as use of both distributions

`pyproject.toml`:

```toml
[project]
dependencies = ["a-lib", "b-lib"]
```

`/venv/<path-to-site-packages>/shared/a.py`:

```py
```

`/venv/<path-to-site-packages>/shared/b.py`:

```py
```

`main.py`:

```py
import shared  # no diagnostic
```

#### Importing a child module only uses its owner

`pyproject.toml`:

```toml
[project] # error: [unused-dependency] "Dependency `b-lib` is declared but never imported"
dependencies = ["a-lib", "b-lib"]
```

`/venv/<path-to-site-packages>/shared/a.py`:

```py
```

`/venv/<path-to-site-packages>/shared/b.py`:

```py
```

`main.py`:

```py
import shared.a  # no diagnostic
```
