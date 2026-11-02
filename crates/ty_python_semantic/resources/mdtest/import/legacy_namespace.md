# Legacy namespace packages

## `__import__("pkg_resources").declare_namespace`

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
__import__("pkg_resources").declare_namespace(__name__)
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
__import__("pkg_resources").declare_namespace(__name__)
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
__import__("pkg_resources").declare_namespace(__name__)
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow import __version__ as airflow_version
from airflow.providers.amazon import __version__ as amazon_provider_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
reveal_type(amazon_provider_version)  # revealed: Literal["9.15.0"]
```

## `__import__("pkgutil").extend_path`

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow import __version__ as airflow_version
from airflow.providers.amazon import __version__ as amazon_provider_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
reveal_type(amazon_provider_version)  # revealed: Literal["9.15.0"]
```

## `pkgutil.extend_path`

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
import pkgutil

__path__ = pkgutil.extend_path(__path__, __name__)
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
import pkgutil

__path__ = pkgutil.extend_path(__path__, __name__)
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
import pkgutil

__path__ = pkgutil.extend_path(__path__, __name__)
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow import __version__ as airflow_version
from airflow.providers.amazon import __version__ as amazon_provider_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
reveal_type(amazon_provider_version)  # revealed: Literal["9.15.0"]
```

## Normalized module identifiers

Python normalizes identifiers with NFKC, so a namespace declaration can refer to `pkgutil` without
its ASCII spelling appearing in the source. The namespace still includes modules from both search
paths.

```toml
[environment]
extra-paths = ["/first", "/second"]
```

`/first/namespace/__init__.py`:

```py
import ｐｋｇｕｔｉｌ

__path__ = ｐｋｇｕｔｉｌ.extend_path(__path__, __name__)
```

`/second/namespace/__init__.py`:

```py
import ｐｋｇｕｔｉｌ

__path__ = ｐｋｇｕｔｉｌ.extend_path(__path__, __name__)
```

`/second/namespace/child.py`:

```py
value = 1
```

`test.py`:

```py
from namespace.child import value

reveal_type(value)  # revealed: Literal[1]
```

## `extend_path` with keyword arguments

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
import pkgutil

__path__ = pkgutil.extend_path(name=__name__, path=__path__)
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
import pkgutil

__path__ = pkgutil.extend_path(name=__name__, path=__path__)
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
import pkgutil

__path__ = pkgutil.extend_path(name=__name__, path=__path__)
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow import __version__ as airflow_version
from airflow.providers.amazon import __version__ as amazon_provider_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
reveal_type(amazon_provider_version)  # revealed: Literal["9.15.0"]
```

## incorrect `__import__` arguments

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
__path__ = __import__("not_pkgutil").extend_path(__path__, __name__)
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
__path__ = __import__("not_pkgutil").extend_path(__path__, __name__)
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
__path__ = __import__("not_pkgutil").extend_path(__path__, __name__)
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow.providers.amazon import __version__ as amazon_provider_version  # error: [unresolved-import]
from airflow import __version__ as airflow_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
```

## incorrect `extend_path` arguments

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
__path__ = __import__("pkgutil").extend_path(__path__, "other_module")
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
__path__ = __import__("pkgutil").extend_path(__path__, "other_module")
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
__path__ = __import__("pkgutil").extend_path(__path__, "other_module")
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow.providers.amazon import __version__ as amazon_provider_version  # error: [unresolved-import]
from airflow import __version__ as airflow_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
```

## incorrect `__import__` arguments for `pkg_resources`

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
__import__("not_pkg_resources").declare_namespace(__name__)
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
__import__("not_pkg_resources").declare_namespace(__name__)
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
__import__("not_pkg_resources").declare_namespace(__name__)
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow.providers.amazon import __version__ as amazon_provider_version  # error: [unresolved-import]
from airflow import __version__ as airflow_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
```

## incorrect `declare_namespace` arguments

```toml
[environment]
extra-paths = ["/airflow-core/src", "/providers/amazon/src/"]
```

`/airflow-core/src/airflow/__init__.py`:

```py
__import__("pkg_resources").declare_namespace("other_module")
__version__ = "3.2.0"
```

`/providers/amazon/src/airflow/__init__.py`:

```py
__import__("pkg_resources").declare_namespace("other_module")
```

`/providers/amazon/src/airflow/providers/__init__.py`:

```py
__import__("pkg_resources").declare_namespace("other_module")
```

`/providers/amazon/src/airflow/providers/amazon/__init__.py`:

```py
__version__ = "9.15.0"
```

`test.py`:

```py
from airflow.providers.amazon import __version__ as amazon_provider_version  # error: [unresolved-import]
from airflow import __version__ as airflow_version

reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
```
