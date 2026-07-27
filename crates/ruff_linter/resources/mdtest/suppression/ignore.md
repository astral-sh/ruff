# Tests for `ruff:ignore` comments

## End-of-line `ruff:ignore` range

These are regression tests for <https://github.com/astral-sh/ruff/issues/25644>, where `ruff:ignore`
comments behaved differently from `noqa` and `ty:ignore` comments when placed on the first line of a
diagnostic range.

```toml
[lint]
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

## `ruff:ignore` comments within a `disable`/`enable` pair

```toml
[lint]
preview = true
select = ["E501", "F401", "RUF100", "RUF103", "RUF104"]
```

An intervening `ruff:ignore` directive shouldn't cause a `disable`/`enable` pair to be reported as
unmatched. Instead, the range suppression should take precedence, and the inner `ruff:ignore` should
be unused, just like a `noqa` comment:

```py
# ruff:disable[F401]
# error: [unused-noqa]
import os  # ruff:ignore[F401]
# ruff:enable[F401]

# ruff:disable[F401]
# error: [unused-noqa]
import sys  # noqa: F401
# ruff:enable[F401]
```

This applies to own-line comments and nested comments too:

```py
# ruff:disable[F401]
# error: [unused-noqa]
# ruff:ignore[F401]
import os

# error: [unused-noqa]
import sys  # noqa: F401
# ruff:enable[F401]

# ruff:disable[F401]
# snapshot: unused-noqa
import sys  # start # ruff:ignore[F401] # end
# ruff:enable[F401]
```

```snapshot
error[RUF100]: Unused suppression (unused: `F401`)
  --> src/mdtest_snippet.py:21:21
   |
21 | import sys  # start # ruff:ignore[F401] # end
   |                     ^^^^^^^^^^^^^^^^^^^^
   |
help: Remove unused suppression
   |
20 | # snapshot: unused-noqa
   - import sys  # start # ruff:ignore[F401] # end
21 + import sys  # start # end
22 | # ruff:enable[F401]
   |
note: This is an unsafe fix and may change runtime behavior
```

and cases where the `disable` and `ignore` suppress different codes:

```py
# ruff:disable[E501]
import os  # ruff:ignore[F401]
message = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
# ruff:enable[E501]
```

This should also work for `ruff:ignore` comments on the same line as the `disable` comment:

```py
def f():
    # error: [unused-noqa] "FIX002"
    # ruff:disable[F401] # ruff:ignore[FIX002]
    import os
    # ruff:enable[F401]
```

## `file-ignore` comments within a `disable`/`enable` pair

```toml
[lint]
select = ["F401", "RUF100", "RUF104"]
```

A `file-ignore` within a range suppression takes precedence and marks the `disable` as unused:

```py
# error: [unused-noqa]
# ruff:disable[F401]
# ruff:file-ignore[F401]
import os
message = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
# ruff:enable[F401]
```

## Disallow human-readable names in stable

```toml
[lint]
preview = false
select = ["F401", "RUF102"]
```

With preview disabled, this should continue to emit `F401`, as well as `RUF102`:

```py
# snapshot: invalid-rule-code
# ruff:disable[unused-import]
# error: [unused-import]
import math
# ruff:enable[unused-import]
```

```snapshot
error[RUF102]: Invalid rule code in suppression: unused-import
 --> src/mdtest_snippet.py:2:16
  |
2 | # ruff:disable[unused-import]
  |                ^^^^^^^^^^^^^
3 | # error: [unused-import]
4 | import math
5 | # ruff:enable[unused-import]
  |               -------------
  |
help: Enable `lint.preview` to use rule names
help: Remove the suppression comment
  |
1 | # snapshot: invalid-rule-code
  - # ruff:disable[unused-import]
2 | # error: [unused-import]
3 | import math
  - # ruff:enable[unused-import]
4 | # snapshot: invalid-rule-code
  |
```

Emit both (non-fix-title) help messages when rule names and unknown codes are present:

```py
# snapshot: invalid-rule-code
# ruff:disable[unused-import, unknown-rule]
# error: [unused-import]
import sys
# ruff:enable[unused-import, unknown-rule]
```

```snapshot
error[RUF102]: Invalid rule code in suppression: unknown-rule, unused-import
  --> src/mdtest_snippet.py:7:1
   |
 7 | # ruff:disable[unused-import, unknown-rule]
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
 8 | # error: [unused-import]
 9 | import sys
10 | # ruff:enable[unused-import, unknown-rule]
   | ------------------------------------------
   |
help: Add non-Ruff rule codes to the `lint.external` configuration option
help: Enable `lint.preview` to use rule names
help: Remove the suppression comment
  |
6 | # snapshot: invalid-rule-code
  - # ruff:disable[unused-import, unknown-rule]
7 | # error: [unused-import]
8 | import sys
  - # ruff:enable[unused-import, unknown-rule]
  |
```

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
# snapshot: invalid-suppression-comment
# ruff:enable[F401]
```

```snapshot
error[RUF103]: Invalid suppression comment: no matching 'disable' comment
  --> src/mdtest_snippet.py:12:1
   |
12 | # ruff:enable[F401]
   | ^^^^^^^^^^^^^^^^^^^
   |
help: Remove suppression comment
   |
11 | # snapshot: invalid-suppression-comment
   - # ruff:enable[F401]
   |
note: This is an unsafe fix and may change runtime behavior
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

### `invalid-rule-code`

Unknown rule names should emit `RUF102`, while preserving valid names in the same suppression:

```py
# snapshot: invalid-rule-code
# ruff:ignore[unused-import, not-a-rule]
import pathlib
```

```snapshot
error[RUF102]: Invalid rule code in suppression: not-a-rule
 --> src/mdtest_snippet.py:2:30
  |
2 | # ruff:ignore[unused-import, not-a-rule]
  |                              ^^^^^^^^^^
  |
help: Add non-Ruff rule codes to the `lint.external` configuration option
help: Remove the rule code `not-a-rule`
  |
1 | # snapshot: invalid-rule-code
  - # ruff:ignore[unused-import, not-a-rule]
2 + # ruff:ignore[unused-import]
3 | import pathlib
  |
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
  |
7 | # snapshot: unused-noqa
  - import math  # ruff:ignore[unused-import]
8 + import math
9 |
  |
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
   |
11 | # snapshot: unused-noqa
   - # ruff:ignore[F401, unused-import]
12 + # ruff:ignore[F401]
13 | import pathlib
   |
```

## Nested comments

```toml
[lint]
select = ["F401", "RUF103", "RUF104"]
```

`ruff:ignore` comments nested within other comments should still work:

```py
import math  # some comment # ruff:ignore[F401] # another comment
```

Invalid suppressions should only delete the suppression part:

```py
# snapshot: invalid-suppression-comment
# error: [unused-import]
import sys  # explanation # ruff:ignore # another
```

```snapshot
error[RUF103]: Invalid suppression comment: missing suppression codes like `[E501, ...]`
 --> src/mdtest_snippet.py:4:27
  |
4 | import sys  # explanation # ruff:ignore # another
  |                           ^^^^^^^^^^^^^^
  |
help: Remove suppression comment
  |
3 | # error: [unused-import]
  - import sys  # explanation # ruff:ignore # another
4 + import sys  # explanation # another
5 | # error: [unmatched-suppression-comment]
  |
note: This is an unsafe fix and may change runtime behavior
```

Nested `disable`/`enable` comments on the same line are invalid. Instead of trying to match them up
to remove them in one operation, they are respectively treated as unmatched (`disable`) and invalid
(trailing `enable`).

```py
# error: [unmatched-suppression-comment]
# error: [invalid-suppression-comment]
# ruff:disable[F401] # ruff:enable[F401]
import foo
```

## Invalid nested block and file comments

```toml
[lint]
select = ["F401", "RUF103", "RUF104"]
```

Nested `disable` and `file-ignore` comments are also invalid and don't suppress diagnostics on the
following line:

```py
# snapshot: invalid-suppression-comment
# explanation # ruff:disable[F401]
# error: [unused-import]
import os

# error: [invalid-suppression-comment]
# explanation # ruff:file-ignore[F401]
# error: [unused-import]
import sys
```

```snapshot
error[RUF103]: Invalid suppression comment: trailing comments are only supported for ruff:ignore suppressions
 --> src/mdtest_snippet.py:2:15
  |
2 | # explanation # ruff:disable[F401]
  |               ^^^^^^^^^^^^^^^^^^^^
  |
help: Remove suppression comment
  |
1 | # snapshot: invalid-suppression-comment
  - # explanation # ruff:disable[F401]
2 + # explanation
3 | # error: [unused-import]
  |
note: This is an unsafe fix and may change runtime behavior
```

Similarly, a nested `enable` is invalid and doesn't re-enable a disabled rule:

```py
# error: [unmatched-suppression-comment]
# ruff:disable[F401]
# error: [invalid-suppression-comment]
# comment # ruff:enable[F401]
import foo
```

## Nested comments on comment-only lines

```toml
[lint]
select = ["F401", "RUF100", "FIX002"]
```

Nested suppression comments on a comment-only line are treated as trailing on the comment itself and
don't suppress diagnostics on the following line:

```py
# error: [unused-noqa]
# explanation # ruff:ignore[F401] # another
# error: [unused-import]
import pathlib

def foo():
    # error: [unused-noqa]
    # explanation # ruff:ignore[F401] # another
    # error: [unused-import]
    import pathlib
```

This can still be useful if the comment itself has a diagnostic:

```py
# TODO this comment has a todo # noqa: FIX002
# TODO this comment has a todo # ruff:ignore[FIX002]
a = 10
```

## `unused-noqa`

```toml
[lint]
select = ["E501", "F821", "RUF100", "RUF103"]
```

`RUF100` should have an unsafe fix when deleting a leading suppression would change the placement
semantics of a later suppression. The `file-ignore` is initially invalid here, but removing the
unused `ignore` would make it valid, so the fix is unsafe:

```py
# snapshot: unused-noqa
# error: [invalid-suppression-comment]
# ruff:ignore[E501] # ruff:file-ignore[F821]
# error: [undefined-name]
undefined_name
```

```snapshot
error[RUF100]: Unused suppression (unused: `E501`)
 --> src/mdtest_snippet.py:3:1
  |
3 | # ruff:ignore[E501] # ruff:file-ignore[F821]
  | ^^^^^^^^^^^^^^^^^^^^
  |
help: Remove unused suppression
  |
2 | # error: [invalid-suppression-comment]
  - # ruff:ignore[E501] # ruff:file-ignore[F821]
3 + # ruff:file-ignore[F821]
4 | # error: [undefined-name]
  |
note: This is an unsafe fix and may change runtime behavior
```

A later valid suppression also needs to be considered because removing the preceding comment can
change its semantics. In this case, removing the `disable` comment would transform the `ignore` from
a trailing comment on the `disable` comment to an own-line `ignore` comment, which would start
suppressing the `F821` diagnostic:

```py
# snapshot: unused-noqa
# error: [unused-noqa] "F821"
# ruff:disable[E501] # ruff:ignore[F821]
# error: [undefined-name]
undefined_name
# ruff:enable[E501]
```

```snapshot
error[RUF100]: Unused suppression (unused: `E501`)
  --> src/mdtest_snippet.py:8:1
   |
 8 | # ruff:disable[E501] # ruff:ignore[F821]
   | ^^^^^^^^^^^^^^^^^^^^^
 9 | # error: [undefined-name]
10 | undefined_name
11 | # ruff:enable[E501]
   | -------------------
   |
help: Remove unused suppression
   |
7  | # error: [unused-noqa] "F821"
   - # ruff:disable[E501] # ruff:ignore[F821]
8  + # ruff:ignore[F821]
9  | # error: [undefined-name]
10 | undefined_name
   - # ruff:enable[E501]
   |
note: This is an unsafe fix and may change runtime behavior
```

## `unused-noqa` paired fix

```toml
[lint]
select = ["E501", "RUF100", "FIX002"]
```

Deleting either half of a `disable`/`enable` pair should make the fix unsafe if in a nested context:

```py
# snapshot: unused-noqa
# ruff:disable[E501]
value = 1
# ruff:enable[E501] # TODO # ruff:ignore[FIX002]
```

```snapshot
error[RUF100]: Unused suppression (unused: `E501`)
 --> src/mdtest_snippet.py:2:1
  |
2 | # ruff:disable[E501]
  | ^^^^^^^^^^^^^^^^^^^^
3 | value = 1
4 | # ruff:enable[E501] # TODO # ruff:ignore[FIX002]
  | --------------------
  |
help: Remove unused suppression
  |
1 | # snapshot: unused-noqa
  - # ruff:disable[E501]
2 | value = 1
  - # ruff:enable[E501] # TODO # ruff:ignore[FIX002]
3 + # TODO # ruff:ignore[FIX002]
  |
note: This is an unsafe fix and may change runtime behavior
```

## `unused-noqa` partial fix

```toml
[lint]
select = ["E501", "F401", "F821", "RUF100", "RUF103"]
```

Removing a code from a multi-code suppression doesn't promote the later suppression, so the fix is
still safe:

```py
# snapshot: unused-noqa
# error: [invalid-suppression-comment]
# ruff:ignore[E501, F821] # ruff:file-ignore[F401]
undefined_name
```

```snapshot
error[RUF100]: Unused suppression (unused: `E501`)
 --> src/mdtest_snippet.py:3:1
  |
3 | # ruff:ignore[E501, F821] # ruff:file-ignore[F401]
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove unused suppression
  |
2 | # error: [invalid-suppression-comment]
  - # ruff:ignore[E501, F821] # ruff:file-ignore[F401]
3 + # ruff:ignore[F821] # ruff:file-ignore[F401]
4 | undefined_name
  |
```

## `invalid-rule-code`

```toml
[lint]
select = ["F821", "RUF102", "RUF103"]
```

The `RUF102` fix should also be unsafe when it would promote a later suppression:

```py
# snapshot: invalid-rule-code
# error: [invalid-suppression-comment]
# ruff:ignore[XYZ] # ruff:file-ignore[F821]
# error: [undefined-name]
undefined_name
```

```snapshot
error[RUF102]: Invalid rule code in suppression: XYZ
 --> src/mdtest_snippet.py:3:15
  |
3 | # ruff:ignore[XYZ] # ruff:file-ignore[F821]
  |               ^^^
  |
help: Add non-Ruff rule codes to the `lint.external` configuration option
help: Remove the suppression comment
  |
2 | # error: [invalid-suppression-comment]
  - # ruff:ignore[XYZ] # ruff:file-ignore[F821]
3 + # ruff:file-ignore[F821]
4 | # error: [undefined-name]
  |
note: This is an unsafe fix and may change runtime behavior
```

## `invalid-suppression-comment`

```toml
[lint]
select = ["F401", "F821", "RUF100", "RUF103"]
```

The same applies to fixes for invalid suppression placement:

```py
def f():
    # snapshot: invalid-suppression-comment
    # error: [unused-noqa]
    # explanation # ruff:file-ignore[F401] # ruff:ignore[F401]
    # error: [unused-import]
    import os
```

```snapshot
error[RUF103]: Invalid suppression comment: trailing comments are only supported for ruff:ignore suppressions
 --> src/mdtest_snippet.py:4:19
  |
4 |     # explanation # ruff:file-ignore[F401] # ruff:ignore[F401]
  |                   ^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove suppression comment
  |
3 |     # error: [unused-noqa]
  -     # explanation # ruff:file-ignore[F401] # ruff:ignore[F401]
4 +     # explanation # ruff:ignore[F401]
5 |     # error: [unused-import]
  |
note: This is an unsafe fix and may change runtime behavior
```

And parse errors:

```py
# snapshot: invalid-suppression-comment
# error: [unused-noqa]
# explanation # ruff:ignore # ruff:ignore[F821]
# error: [undefined-name]
undefined_name
```

```snapshot
error[RUF103]: Invalid suppression comment: missing suppression codes like `[E501, ...]`
 --> src/mdtest_snippet.py:9:15
  |
9 | # explanation # ruff:ignore # ruff:ignore[F821]
  |               ^^^^^^^^^^^^^^
  |
help: Remove suppression comment
   |
8  | # error: [unused-noqa]
   - # explanation # ruff:ignore # ruff:ignore[F821]
9  + # explanation # ruff:ignore[F821]
10 | # error: [undefined-name]
   |
note: This is an unsafe fix and may change runtime behavior
```

## Invalid nested comments

```toml
[lint]
select = ["RUF103"]
```

Parse errors should only highlight and delete the nested suppression:

```py
# snapshot: invalid-suppression-comment
import os  # explanation # ruff:unknown[F401] # another
# snapshot: invalid-suppression-comment
import sys  # explanation # ruff:ignore[F401 F841] # another
```

```snapshot
error[RUF103]: Invalid suppression comment: unknown ruff directive
 --> src/mdtest_snippet.py:2:26
  |
2 | import os  # explanation # ruff:unknown[F401] # another
  |                          ^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove suppression comment
  |
1 | # snapshot: invalid-suppression-comment
  - import os  # explanation # ruff:unknown[F401] # another
2 + import os  # explanation # another
3 | # snapshot: invalid-suppression-comment
  |
note: This is an unsafe fix and may change runtime behavior


error[RUF103]: Invalid suppression comment: missing comma between codes
 --> src/mdtest_snippet.py:4:27
  |
4 | import sys  # explanation # ruff:ignore[F401 F841] # another
  |                           ^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove suppression comment
  |
3 | # snapshot: invalid-suppression-comment
  - import sys  # explanation # ruff:ignore[F401 F841] # another
4 + import sys  # explanation # another
  |
note: This is an unsafe fix and may change runtime behavior
```

## Recovery after invalid nested comments

```toml
[lint]
select = ["F401", "RUF103"]
```

A malformed nested suppression should not prevent a later valid suppression from being parsed:

```py
# error: [invalid-suppression-comment]
import os  # before # ruff:ignore # ruff:ignore[F401] # after
```

## Fixing unused nested comments

```toml
[lint]
select = ["RUF100"]
```

Fixes for unused nested suppressions should preserve the surrounding comment fragments:

```py
# snapshot: unused-noqa
value = 1  # before # ruff:ignore[F401] # after

# snapshot: unused-noqa
value = 1  # before # ruff:ignore[F401]
```

```snapshot
error[RUF100]: Unused suppression (non-enabled: `F401`)
 --> src/mdtest_snippet.py:2:21
  |
2 | value = 1  # before # ruff:ignore[F401] # after
  |                     ^^^^^^^^^^^^^^^^^^^^
  |
help: Remove unused suppression
  |
1 | # snapshot: unused-noqa
  - value = 1  # before # ruff:ignore[F401] # after
2 + value = 1  # before # after
3 |
  |
note: This is an unsafe fix and may change runtime behavior


error[RUF100]: Unused suppression (non-enabled: `F401`)
 --> src/mdtest_snippet.py:5:21
  |
5 | value = 1  # before # ruff:ignore[F401]
  |                     ^^^^^^^^^^^^^^^^^^^
  |
help: Remove unused suppression
  |
4 | # snapshot: unused-noqa
  - value = 1  # before # ruff:ignore[F401]
5 + value = 1  # before
  |
note: This is an unsafe fix and may change runtime behavior
```

## Empty diagnostic range before a shebang

An ignore comment at the start of the second line should suppress diagnostics with an empty range
at offset zero, as with `noqa`.

```toml
[lint]
select = ["D100"]
```

```py
#!/usr/bin/env python
# ruff:ignore[D100]
```
