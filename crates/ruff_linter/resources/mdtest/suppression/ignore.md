# Tests for `ruff:ignore` comments

## End-of-line `ruff:ignore` range

These are regression tests for <https://github.com/astral-sh/ruff/issues/25644>, where `ruff:ignore`
comments behaved differently from `noqa` and `ty:ignore` comments when placed on the first line of a
diagnostic range.

```toml
[lint]
preview = true
select = ["RUF015"]
```

```py
suppressed = [  # noqa: RUF015
    *range(10)
][0]

not_suppressed = [  # ruff:ignore[RUF015]
    *range(10)
][0]
```

## Standalone `ruff:ignore` range

This case also was previously not suppressed but will be now. The `B903` diagnostic covers the
entire class definition.

```toml
[lint]
preview = true
select = ["B903"]
```

```py
# ruff:ignore[B903]
class Point:
    def __init__(self, x: int):
        self.x = x
```

## Empty diagnostic range at end of file

Diagnostics with empty ranges should also be suppressible, as with `noqa`.

```toml
[lint]
preview = true
select = ["W292"]
```

```py
suppressed = 1  # ruff:ignore[W292]
```

## Block suppression boundaries

A block suppression should not apply to a diagnostic that starts inside the disabled range but ends
after the matching `ruff:enable` comment:

```toml
[lint]
preview = true
select = ["RUF015"]
```

```py
# ruff:disable[RUF015]
# error: [unnecessary-iterable-allocation-for-first-element]
not_suppressed = [
# ruff:enable[RUF015]
    *range(10)
][0]
```

This isn't _strictly_ a problem because the formatter will indent and invalidate the `ruff:enable`
comment here, raising `RUF103`, but it seems better for this not to work regardless of formatting.

This is how the comments should look instead:

```py
# ruff:disable[RUF015]
not_suppressed = [
    *range(10)
][0]
# ruff:enable[RUF015]
```

## Make sure the code actually matches

```toml
[lint]
preview = true
select = ["RUF015"]
```

This case tests that both the range and suppression code are checked.

```py
# error: [unnecessary-iterable-allocation-for-first-element]
not_suppressed = [  # ruff:ignore[F401]
    *range(10)
][0]
```

## Own-line ignore covers trailing comments

```toml
[lint]
preview = true
select = [ "E" ]
```

This should be suppressed:

```py
# ruff:ignore[E262]
x = 1  #bad
```

An ignore comment inside a multi-line statement only covers the next physical line, including its
trailing comment:

```py
values = [
    # ruff:ignore[E262]
    1,  #bad
    # error: [no-space-after-inline-comment]
    2,  #bad
]
```

An own-line ignore does not extend to a comment on the following line:

```py
# ruff:ignore[E265]
x = 1
# error: [no-space-after-block-comment]
#bad
```

An own-line ignore above a multi-line statement covers a trailing comment on its final line:

```py
# ruff:ignore[E262]
x = (
    1
)  #bad
```

## Respect parent suppression range

```toml
[lint]
preview = true
select = [ "F" ]
```

Some diagnostics have a "parent" range, which should also be accounted for when suppressing them
with both `noqa` and `ruff:ignore`.

```py
from foo import (  # noqa: F401
        bar
)

from foo import (  # ruff:ignore[F401]
        baz
)
```

## Parent suppression range and unused comments

```toml
[lint]
preview = true
select = [ "F401", "RUF100" ]
```

For cases with both a parent and non-parent `noqa` comment, the parent is marked as used, while the
non-parent, which falls later textually, is marked as unused.

```py
from math import ( # noqa: F401
    # error: [unused-noqa]
    cos # noqa: F401
)
```

`ruff:ignore` should behave in the same way:

```py
from sys import ( # ruff:ignore[F401]
    # error: [unused-noqa]
    argv # ruff:ignore[F401]
)
```

## Trailing whitespace is included in the suppression range

```toml
[lint]
preview = true
select = [ "W291" ]
```

For both logical and non-logical newlines:

<!-- fmt:off -->

```py
# ruff:ignore[W291]
foo    

values = [
    # ruff:ignore[W291]
    bar    
]
```

<!-- fmt:on -->

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

The `disable` and `enable` comments must match textually, even when a rule code and name identify
the same rule:

```py
# error: [unmatched-suppression-comment]
# ruff:disable[unused-import]
import math
# error: [invalid-suppression-comment]
# ruff:enable[F401]
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

Unused suppressions with rule codes should still emit `RUF100` with an appropriate error message:

```py
# error: [unused-noqa]
import math  # noqa: F401

# error: [unused-noqa]
import math  # ruff:ignore[F401]

# snapshot: unused-noqa
import math  # ruff:ignore[unused-import]

math.cos(1)
```

```snapshot
error[RUF100]: Unused suppression (unused: `unused-import`)
 --> src/mdtest_snippet.py:8:14
  |
8 | import math  # ruff:ignore[unused-import]
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove unused suppression
5  | import math  # ruff:ignore[F401]
6  |
7  | # snapshot: unused-noqa
   - import math  # ruff:ignore[unused-import]
8  + import math
9  |
10 | math.cos(1)
11 | # snapshot: unused-noqa
```

A rule code and human-readable name for the same rule are treated as separate suppressions. The
second suppression is therefore unused rather than duplicated:

```py
# snapshot: unused-noqa
# ruff:ignore[F401, unused-import]
import pathlib
```

```snapshot
error[RUF100]: Unused suppression (unused: `unused-import`)
  --> src/mdtest_snippet.py:12:1
   |
12 | # ruff:ignore[F401, unused-import]
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
help: Remove unused suppression
9  |
10 | math.cos(1)
11 | # snapshot: unused-noqa
   - # ruff:ignore[F401, unused-import]
12 + # ruff:ignore[F401]
13 | import pathlib
```
