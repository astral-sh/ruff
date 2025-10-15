# External packages

This document shows how external dependencies can be used in Markdown-based tests, and makes sure
that we can correctly use some common packages. See the mdtest README for more information.

## pydantic

```toml
[environment]
python-version = "3.13"

[project]
dependencies = ["pydantic==2.12.2"]
```

```py
import pydantic

reveal_type(pydantic.__version__)  # revealed: Literal["2.12.2"]
```

## numpy

```toml
[environment]
python-version = "3.13"

[project]
dependencies = ["numpy==2.3.0"]
```

```py
import numpy as np

reveal_type(np.float64)  # revealed: <class 'float64'>
```

## requests

```toml
[environment]
python-version = "3.13"

[project]
dependencies = ["requests==2.32.5"]
```

```py
import requests

reveal_type(requests.__version__)  # revealed: Literal["2.32.5"]
```

## pytest

```toml
[environment]
python-version = "3.13"

[project]
dependencies = ["pytest==8.4.2"]
```

```py
import pytest

reveal_type(pytest.fail)  # revealed: _WithException[Unknown, <class 'Failed'>]
```
