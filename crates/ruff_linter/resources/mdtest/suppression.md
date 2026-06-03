# Tests for suppression comments

## Allow human-readable names in preview

Enable preview, `unused-import` and several `RUF` rules to check for valid suppression comments:

```toml
[lint]
preview = true
select = ["F401", "RUF100", "RUF102", "RUF103", "RUF104"]
```

This comment should suppress the `F401` diagnostic and not emit any other errors:

```py
# ruff:ignore[unused-import]
import math
```

