# Tests for suppression comments

## Allow human-readable names in preview

Enable preview, `unused-import` and several `RUF` rules to check for valid suppression comments:

```toml
[lint]
preview = true
select = ["F401", "RUF100", "RUF102", "RUF103", "RUF104"]
```

### `ruff:ignore`

This comment should suppress the `F401` diagnostic and not emit any other errors:

```py
# ruff:ignore[unused-import]
import math
```

### `ruff:file-ignore`

File-level ignores should also work:

```py
# ruff:file-ignore[unused-import]

import math
import sys
import traceback
```

### `ruff:disable`

As should block-level ignores:

```py
import math  # error: [unused-import]

# ruff:disable[unused-import]
import sys
# ruff:enable[unused-import]

import traceback  # error: [unused-import]
```

### `noqa`

Old-style `noqa` comments should continue to reject rule names:

```py
# error: [unused-import]
import math  # noqa: unused-import
```

but obviously continue working with rule codes:

```py
import math  # noqa: F401
```

### `unused-noqa`

Unused suppressions with rule codes should still emit `RUF100`:

```py
# error: [unused-noqa]
import math  # noqa: F401

# error: [unused-noqa]
import math  # ruff:ignore[F401]

# error: [unused-noqa]
import math  # ruff:ignore[unused-import]

math.cos(1)
```
