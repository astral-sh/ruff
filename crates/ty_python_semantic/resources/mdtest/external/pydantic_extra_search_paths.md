# Pydantic on extra search paths

Pydantic-specific behavior still applies when the installed package is resolved from an extra search
path, such as when its `site-packages` directory is included in `PYTHONPATH`.

```toml
[environment]
python-version = "3.11"
python-platform = "linux"
extra-paths = ["/.venv/<path-to-site-packages>"]

[project]
dependencies = ["pydantic==2.13.4"]
```

```py
from pydantic import BaseModel, ConfigDict

class Model(BaseModel):
    model_config = ConfigDict(extra="allow")

Model(a=1)
```
