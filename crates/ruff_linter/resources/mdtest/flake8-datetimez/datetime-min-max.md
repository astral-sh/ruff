# `datetime-min-max` (`DTZ901`)

```toml
lint.select = ["DTZ901"]
```

## Replacing the timezone with None

Passing `tzinfo=None` to `replace` leaves `datetime.min` and `datetime.max` naive.

```py
from datetime import datetime

datetime.min.replace(tzinfo=None)  # error: [datetime-min-max]
datetime.max.replace(tzinfo=None)  # error: [datetime-min-max]
```

## Replacing the timezone with a timezone object

Passing a timezone object produces an aware datetime.

```py
from datetime import datetime, timezone

datetime.min.replace(tzinfo=timezone.utc)
datetime.max.replace(tzinfo=timezone.utc)
```
