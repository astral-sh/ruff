## Python version 3.15

```toml
[environment]
python-version = "3.15"
```

```py
import decimal, time

time.ctime(decimal.Decimal("1.5"))  # invalid on 3.14, supported on 3.15
```
