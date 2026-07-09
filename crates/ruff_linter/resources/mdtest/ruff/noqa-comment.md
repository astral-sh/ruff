# `noqa-comment` (`RUF105`)

```toml
[lint]
preview = true
select = ["noqa-comment", "F401", "F402", "F403"]
```

## File-level comments

### Single code

```py
# snapshot: noqa-comment
# ruff: noqa: F401
import math
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401
  | ^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401
2 + # ruff:file-ignore[F401]
3 | import math
  |
```

### Multiple codes

```py
# snapshot: noqa-comment
# ruff: noqa: F401, F402, F403
import math
import os
from module import *
for os in []:
    pass
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401, F402, F403
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401, F402, F403
2 + # ruff:file-ignore[F401, F402, F403]
3 | import math
  |
```

### Multiple codes followed by a reason

```py
# snapshot: noqa-comment
# ruff: noqa: F401, F402, F403 for some reason
import math
import os
from module import *
for os in []:
    pass
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401, F402, F403 for some reason
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401, F402, F403 for some reason
2 + # ruff:file-ignore[F401, F402, F403] for some reason
3 | import math
  |
```

### Multiple codes followed by a nested (pragma) comment

```py
# snapshot: noqa-comment
# ruff: noqa: F401, F402, F403 # fmt:skip
import math
import os
from module import *
for os in []:
    pass
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa: F401, F402, F403 # fmt:skip
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:file-ignore` instead
  |
1 | # snapshot: noqa-comment
  - # ruff: noqa: F401, F402, F403 # fmt:skip
2 + # ruff:file-ignore[F401, F402, F403] # fmt:skip
3 | import math
  |
```

### Unknown codes still receive a diagnostic

In case the unknown code is a typo rather than an intentionally external code, we emit both
`invalid-rule-code` and `noqa-comment`:

```toml
[lint]
preview = true
select = ["noqa-comment", "unused-noqa", "invalid-rule-code", "F401"]
```

```py
# error: [invalid-rule-code]
# snapshot: noqa-comment
import math  # noqa: F401, UNK001
```

```snapshot
error[RUF105]: `noqa` comment used instead of `ruff:ignore`
 --> src/mdtest_snippet.py:3:14
  |
3 | import math  # noqa: F401, UNK001
  |              ^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:ignore` instead
  |
2 | # snapshot: noqa-comment
  - import math  # noqa: F401, UNK001
3 + import math  # ruff:ignore[F401, UNK001]
  |
```

### External codes

```toml
[lint]
preview = true
select = ["noqa-comment", "unused-noqa", "invalid-rule-code", "F401"]
external = ["EXT"]
```

If all of the codes are marked `external`, no diagnostic is emitted:

```py
# error: [unused-import]
import math  # noqa: EXT001, EXT002
```

However, if only some of the codes are `external`, a diagnostic is emitted without an autofix. In
this case, the external codes likely need to remain in a `noqa` comment, while the codes known by
Ruff could potentially move into a `ruff:ignore` comment.

```py
# snapshot: noqa-comment
import math  # noqa: F401, EXT001
```

```snapshot
error[RUF105]: `noqa` comment used instead of `ruff:ignore`
 --> src/mdtest_snippet.py:4:14
  |
4 | import math  # noqa: F401, EXT001
  |              ^^^^^^^^^^^^^^^^^^^^
  |
help: Use `ruff:ignore` instead
```

### Any unmatched code disables the rule

This leaves an unused `noqa` comment to be cleaned up by `RUF100` instead, which can be especially
important in the case of a standalone `noqa` comment, which has no effect (in almost all cases), but
could become an effectful own-line `ruff:ignore` comment if `RUF105` applied.

```py
# ruff: noqa: F401, F402
import math
```

### Flake8 comments are ignored

```py
# flake8: noqa: F401
import math
```

## Inline comments

### Basic

```py
# snapshot: noqa-comment
import math  # noqa: F401

import os  # noqa: F401, F402
```

```snapshot
error[RUF105]: `noqa` comment used instead of `ruff:ignore`
 --> src/mdtest_snippet.py:2:14
  |
2 | import math  # noqa: F401
  |              ^^^^^^^^^^^^
  |
help: Use `ruff:ignore` instead
  |
1 | # snapshot: noqa-comment
  - import math  # noqa: F401
2 + import math  # ruff:ignore[F401]
3 |
  |
```

### Nested pragma comment before the directive

```py
# snapshot: noqa-comment
import math  # fmt:skip # noqa: F401
```

```snapshot
error[RUF105]: `noqa` comment used instead of `ruff:ignore`
 --> src/mdtest_snippet.py:2:25
  |
2 | import math  # fmt:skip # noqa: F401
  |                         ^^^^^^^^^^^^
  |
help: Use `ruff:ignore` instead
  |
1 | # snapshot: noqa-comment
  - import math  # fmt:skip # noqa: F401
2 + import math  # fmt:skip # ruff:ignore[F401]
  |
```

## Blanket comments

RUF105 flags blanket comments but does not offer a fix because `ruff:ignore` requires at least one
rule selector.

### Inline

```py
# snapshot: noqa-comment
import math  # noqa
```

```snapshot
error[RUF105]: `noqa` comment used instead of `ruff:ignore`
 --> src/mdtest_snippet.py:2:14
  |
2 | import math  # noqa
  |              ^^^^^^
  |
```

### File-level

```py
# snapshot: noqa-comment
# ruff: noqa
import math
```

```snapshot
error[RUF105]: `ruff: noqa` comment used instead of `ruff:file-ignore`
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff: noqa
  | ^^^^^^^^^^^^
  |
```

## Inline self-suppression

```toml
[lint]
preview = true
select = ["noqa-comment", "unused-noqa", "F401"]
```

It should be possible to suppress `RUF105` with a `noqa` comment:

```py
value = 1  # noqa: RUF105
```

But a suppression for `RUF100` should not prevent the rule from firing:

```py
# error: [noqa-comment]
import math  # noqa: RUF100, F401
```

## Suppression with `ruff:ignore`

```toml
[lint]
preview = true
select = ["noqa-comment", "unused-noqa", "F401"]
```

### Inline suppression

```py
import math  # noqa: F401  # ruff:ignore[RUF105]
```

### Standalone suppression

```py
# ruff:ignore[RUF105]
# ruff: noqa: F401
import math
```

### File-level suppression

```py
# ruff:file-ignore[RUF105]
# ruff: noqa: F401
import math
```
