# Missing type stubs

The `missing-type-stubs` rule reports imports of third-party modules that resolve to source files
without a `py.typed` marker or stubs.

## Disabled by default

```toml
[environment]
python = ".venv"
python-version = "3.13"
```

`/.venv/<path-to-site-packages>/untyped_package/__init__.py`:

```py
VALUE = 1
```

`main.py`:

```py
import untyped_package
```

## Untyped package

```toml
[environment]
python = ".venv"
python-version = "3.13"

[rules]
missing-type-stubs = "error"
```

`/.venv/<path-to-site-packages>/untyped_package/__init__.py`:

```py
VALUE = 1
```

`main.py`:

```py
import untyped_package  # error: [missing-type-stubs] "No type stubs found for module `untyped_package`"
```

## Untyped module

```toml
[environment]
python = ".venv"
python-version = "3.13"

[rules]
missing-type-stubs = "error"
```

`/.venv/<path-to-site-packages>/untyped_module.py`:

```py
VALUE = 1
```

`main.py`:

```py
import untyped_module  # error: [missing-type-stubs] "No type stubs found for module `untyped_module`"
```

## Import from untyped package

```toml
[environment]
python = ".venv"
python-version = "3.13"

[rules]
missing-type-stubs = "error"
```

`/.venv/<path-to-site-packages>/untyped_package/__init__.py`:

```py
VALUE = 1
```

`main.py`:

```py
from untyped_package import VALUE  # error: [missing-type-stubs] "No type stubs found for module `untyped_package`"
```

## Typed package

```toml
[environment]
python = ".venv"
python-version = "3.13"

[rules]
missing-type-stubs = "error"
```

`/.venv/<path-to-site-packages>/typed_package/py.typed`:

```text
```

`/.venv/<path-to-site-packages>/typed_package/__init__.py`:

```py
VALUE = 1
```

`main.py`:

```py
import typed_package
```

## Side-by-side stub

```toml
[environment]
python = ".venv"
python-version = "3.13"

[rules]
missing-type-stubs = "error"
```

`/.venv/<path-to-site-packages>/stubbed_module.py`:

```py
VALUE = 1
```

`/.venv/<path-to-site-packages>/stubbed_module.pyi`:

```pyi
VALUE: int
```

`main.py`:

```py
import stubbed_module
```

## Stub package

```toml
[environment]
python = ".venv"
python-version = "3.13"

[rules]
missing-type-stubs = "error"
```

`/.venv/<path-to-site-packages>/stubbed_package-stubs/__init__.pyi`:

```pyi
VALUE: int
```

`/.venv/<path-to-site-packages>/stubbed_package/__init__.py`:

```py
VALUE = 1
```

`main.py`:

```py
import stubbed_package
```

## First-party module

```toml
[rules]
missing-type-stubs = "error"
```

`untyped_first_party.py`:

```py
VALUE = 1
```

`main.py`:

```py
import untyped_first_party
```

## Editable install

```toml
[environment]
python = ".venv"
python-version = "3.13"

[rules]
missing-type-stubs = "error"
```

`/.venv/<path-to-site-packages>/editable.pth`:

```text
/editable
```

`/editable/editable_package/__init__.py`:

```py
VALUE = 1
```

`main.py`:

```py
import editable_package  # error: [missing-type-stubs] "No type stubs found for module `editable_package`"
```
