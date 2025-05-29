# Tests for `site-packages` discovery

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
home = /doo/doo/wop/cpython-3.13.2-macos-aarch64-none/bin
implementation = CPython
uv = 0.7.6
version_info = 3.13.2
include-system-site-packages = false
prompt = ruff
extends-environment = /.other-environment
```

`/doo/doo/wop/cpython-3.13.2-macos-aarch64-none/bin/python`:

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
