# Namespace package

## Basic namespace package

```toml
[environment]
python = "/.venv"
```

`parent/child/one.py`:

```py
one = 1
```

`/.venv/<path-to-site-packages>/parent/child/two.py`:

```py
two = 2
```

`main.py`:

```py
import parent.child.one
import parent.child.two
```

`from.py`

```py
# TODO: This should not be an error
from parent.child import one, two  # error: [unresolved-import]
```

## Regular package in namespace package

```toml
[environment]
python = "/.venv"
```

An adapted test case from the
[PEP420 examples](https://peps.python.org/pep-0420/#nested-namespace-packages). The
`src/parent/child` package is a regular package. Therefore, `site_packages/parent/child/two.py`
should not be resolved.

```ignore
src
  parent
    child
      __init__.py
      one.py
.venv/site-packages
  parent
    child
      two.py
```

`parent/child/__init__.py`:

```py
```

`parent/child/one.py`:

```py
one = 1
```

`/.venv/<path-to-site-packages>/parent/child/two.py`:

```py
two = 2
```

`main.py`:

```py
import parent.child.one

import parent.child.two  # error: [unresolved-import]
```

## Priority between file and identically named namespace package

If there's a namespace package with the same name as a module, the module takes precedence.

`foo.py`:

```py
x = "module"
```

`foo/bar.py`:

```py
x = "namespace"
```

```py
from foo import x

reveal_type(x)  # revealed: Unknown | Literal["module"]

import foo.bar  # error: [unresolved-import]
```
