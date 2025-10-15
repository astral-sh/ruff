# Legacy namespace packages

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
from airflow.providers.amazon import __version__ as amazon_provider_verison
from airflow import __version__ as airflow_version

reveal_type(amazon_provider_verison)  # revealed: Literal["9.15.0"]
reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
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
from airflow.providers.amazon import __version__ as amazon_provider_verison
from airflow import __version__ as airflow_version

reveal_type(amazon_provider_verison)  # revealed: Literal["9.15.0"]
reveal_type(airflow_version)  # revealed: Literal["3.2.0"]
```
