# pandas

```toml
[environment]
python-version = "3.11"
python-platform = "linux"

[project]
dependencies = ["pandas-stubs==3.0.3.260530"]
```

## Series sum with generic protocol self type

`pandas.Series.sum` uses a generic protocol as its `self` annotation. This used to recurse through
constraint-set implication checks while applying deferred quantification.

```py
from pandas import Series

def f(s: Series):
    reveal_type(s.sum())  # revealed: Unknown
```
