# `call-datetime-strptime-without-zone` (`DTZ007`)

```toml
lint.select = ["DTZ007"]
```

## Replacing fields before converting the timezone

Calling `astimezone` produces an aware datetime even when one or more `replace` calls precede it.

```py
from datetime import datetime, timezone

datetime.strptime("2026", "%Y").replace(microsecond=0).astimezone()
datetime.strptime("2026", "%Y").replace(hour=12).replace(minute=30).astimezone(timezone.utc)
datetime.strptime("2026", "%Y").replace(tzinfo=None).astimezone()
```

## Replacements that leave a naive datetime

A replacement without a timezone conversion still produces a naive datetime.

```py
from datetime import datetime

datetime.strptime("2026", "%Y").replace(microsecond=0)  # error: [call-datetime-strptime-without-zone]
datetime.strptime("2026", "%Y").replace(tzinfo=None)  # error: [call-datetime-strptime-without-zone]
```

## Passing a method to another call

Passing the `replace` method to another object's method does not convert the parsed datetime.

```py
from datetime import datetime

def convert(converter):
    converter.replace(datetime.strptime("2026", "%Y").replace).astimezone()  # error: [call-datetime-strptime-without-zone]
```
