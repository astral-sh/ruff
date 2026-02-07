# Setting the target Python version

This file just makes sure that we can configure the target Python version and recognize
version-specific features.

## Python version 3.9

```toml
[environment]
python-version = "3.9"
```

Not valid in 3.9, only in 3.10 and later:

```py
(3).bit_count()  # error: [unresolved-attribute]
```

## Python version 3.10

```toml
[environment]
python-version = "3.10"
```

Valid in 3.10:

```py
(3).bit_count()  # no error
```

Not valid in 3.10, only in 3.11 and later:

```py
import tomllib  # error: [unresolved-import]
```

## Python version 3.11

```toml
[environment]
python-version = "3.11"
```

Valid in 3.11:

```py
import tomllib  # no error
```

Not valid in 3.11, only in 3.12 and later:

```py
from collections.abc import Buffer  # error: [unresolved-import]
```

## Python version 3.12

```toml
[environment]
python-version = "3.12"
```

Valid in 3.12:

```py
from collections.abc import Buffer  # no error
```

Not valid in 3.12, only in 3.13 and later:

```py
from copy import replace  # error: [unresolved-import]
```

## Python version 3.13

```toml
[environment]
python-version = "3.13"
```

Valid in 3.13:

```py
from copy import replace  # no error
```

Not valid in 3.13, only in 3.14 and later:

```py
from compression import zstd  # error: [unresolved-import]
```

## Python version 3.14

```toml
[environment]
python-version = "3.14"
```

Valid in 3.14:

```py
from compression import zstd  # no error
```

Not valid in 3.14, only in 3.15 and later:

```py
import decimal, time

time.ctime(decimal.Decimal("1.5"))  # error: [invalid-argument-type]
```

## Python version 3.15

```toml
[environment]
python-version = "3.15"
```

Valid in 3.15:

```py
import decimal, time

time.ctime(decimal.Decimal("1.5"))  # no error
```
