# Tests for `site-packages` discovery

## Malformed or absent `version` fields

The `version`/`version_info` key in a `pyvenv.cfg` file is provided by most virtual-environment
creation tools to indicate the Python version the virtual environment is for. They key is useful for
our purposes, so we try to parse it when possible. However, the key is not read by the CPython
standard library, and is provided under different keys depending on which virtual-environment
creation tool created the `pyvenv.cfg` file (the stdlib `venv` module calls the key `version`,
whereas uv and virtualenv both call it `version_info`). We therefore do not return an error when
discovering a virtual environment's `site-packages` directory if the virtula environment contains a
`pyvenv.cfg` file which doesn't have this key, or if the associated value of the key doesn't parse
according to our expectations. The file isn't really *invalid* in this situation.

### No `version` field

```toml
[environment]
python = "/.venv"
```

`/.venv/pyvenv.cfg`:

```cfg
home = /do/re/mi//cpython-3.13.2-macos-aarch64-none/bin
```

`/do/re/mi//cpython-3.13.2-macos-aarch64-none/bin/python`:

```text
```

`/.venv/<path-to-site-packages>/foo.py`:

```py
X: int = 42
```

`/src/main.py`:

```py
from foo import X

reveal_type(X)  # revealed: int
```

### Malformed stdlib-style version field

```toml
[environment]
python = "/.venv"
```

`/.venv/pyvenv.cfg`:

```cfg
home = /do/re/mi//cpython-3.13.2-macos-aarch64-none/bin
version = wut
```

`/do/re/mi//cpython-3.13.2-macos-aarch64-none/bin/python`:

```text
```

`/.venv/<path-to-site-packages>/foo.py`:

```py
X: int = 42
```

`/src/main.py`:

```py
from foo import X

reveal_type(X)  # revealed: int
```

### Malformed uv-style version field

```toml
[environment]
python = "/.venv"
```

`/.venv/pyvenv.cfg`:

```cfg
home = /do/re/mi//cpython-3.13.2-macos-aarch64-none/bin
version_info = no-really-wut
```

`/do/re/mi//cpython-3.13.2-macos-aarch64-none/bin/python`:

```text
```

`/.venv/<path-to-site-packages>/foo.py`:

```py
X: int = 42
```

`/src/main.py`:

```py
from foo import X

reveal_type(X)  # revealed: int
```

## Ephemeral uv environments

If you use the `--with` flag when invoking `uv run`, uv will create an "ephemeral" virtual
environment that is layered on top of the pre-existing environment. `site-packages` directories from
the pre-existing environment will be added as an import search path at runtime as well as the
`site-packages` directory from the ephemeral environment. The `VIRTUAL_ENV` environment variable
will only point to the ephemeral virtual environment, but, following uv commit
`7bba3d00d4ad1fb3daba86b98eb25d8d9e9836ae`, uv writes the `sys.prefix` path of the parent
environment to an `extends-environment` key in the ephemeral environment's `pyvenv.cfg` file.

This test ensures that we are able to resolve imports that point to packages in either
`site-packages` directory (the one of the ephemeral environment or the one of the parent
environment) if we detect that an ephemeral uv environment has been activated.

```toml
[environment]
python = "/.venv"
```

`/.venv/pyvenv.cfg`:

```cfg
home = /do/re/mi//cpython-3.13.2-macos-aarch64-none/bin
implementation = CPython
uv = 0.7.6
version_info = 3.13.2
include-system-site-packages = false
prompt = ruff
extends-environment = /.other-environment
```

`/do/re/mi//cpython-3.13.2-macos-aarch64-none/bin/python`:

```text
```

`/.venv/<path-to-site-packages>/foo.py`:

```py
X: int = 42
```

`/.other-environment/<path-to-site-packages>/bar.py`:

```py
Y: "str" = "Y"
```

`/src/main.py`:

```py
from foo import X
from bar import Y

reveal_type(X)  # revealed: int
reveal_type(Y)  # revealed: str
```

## `pyvenv.cfg` files with unusual values

`pyvenv.cfg` files can have unusual values in them, which can contain arbitrary characters. This
includes `=` characters. The following is a regression test for
<https://github.com/astral-sh/ty/issues/430>.

```toml
[environment]
python = "/.venv"
```

`/.venv/pyvenv.cfg`:

```cfg
home = /do/re/mi//cpython-3.13.2-macos-aarch64-none/bin
version_info = 3.13
command = /.pyenv/versions/3.13.3/bin/python3.13 -m venv --without-pip --prompt="python-default/3.13.3" /somewhere-else/python/virtualenvs/python-default/3.13.3
```

`/do/re/mi//cpython-3.13.2-macos-aarch64-none/bin/python`:

```text
```

`/.venv/<path-to-site-packages>/foo.py`:

```py
X: int = 42
```

`/src/main.py`:

```py
from foo import X

reveal_type(X)  # revealed: int
```
