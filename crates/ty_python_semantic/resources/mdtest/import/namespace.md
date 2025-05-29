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
from parent.child import one, two

reveal_type(one)  # revealed: <module 'parent.child.one'>
reveal_type(two)  # revealed: <module 'parent.child.two'>
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

## `from` import with namespace package

Regression test for <https://github.com/astral-sh/ty/issues/363>

`google/cloud/pubsub_v1/__init__.py`:

```py
class PublisherClient: ...
```

```py
from google.cloud import pubsub_v1

reveal_type(pubsub_v1.PublisherClient)  # revealed: <class 'PublisherClient'>
```

## `from` root importing sub-packages

Regresssion test for <https://github.com/astral-sh/ty/issues/375>

`opentelemetry/trace/__init__.py`:

```py
class Trace: ...
```

`opentelemetry/metrics/__init__.py`:

```py
class Metric: ...
```

```py
from opentelemetry import trace, metrics

reveal_type(trace)  # revealed: <module 'opentelemetry.trace'>
reveal_type(metrics)  # revealed: <module 'opentelemetry.metrics'>
```
