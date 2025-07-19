# `static_assert`

## Diagnostics

<!-- snapshot-diagnostics -->

```py
from ty_extensions import static_assert
import secrets

# a passing assert
static_assert(1 < 2)

# evaluates to False
# error: [static-assert-error]
static_assert(1 > 2)

# evaluates to False, with a message as the second argument
# error: [static-assert-error]
static_assert(1 > 2, "with a message")

# evaluates to something falsey
# error: [static-assert-error]
static_assert("")

# evaluates to something ambiguous
# error: [static-assert-error]
static_assert(secrets.randbelow(2))
```
