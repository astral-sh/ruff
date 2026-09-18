# Import fixes

The shared import-editing helpers preserve explicit `lazy` syntax when extending, moving, or replacing
imports.

```toml
target-version = "py315"

[lint]
preview = true
select = ["UP017", "TC004", "AIR321"]
```

## Extending an import

`UP017` adds `UTC` to the existing lazy import.

```py
lazy from datetime import timezone

timezone.utc  # snapshot: datetime-timezone-utc
```

```snapshot
error[UP017]: Use `datetime.UTC` alias
 --> src/mdtest_snippet.py:3:1
  |
3 | timezone.utc  # snapshot: datetime-timezone-utc
  | ^^^^^^^^^^^^
help: Convert to `datetime.UTC` alias
  |
  - lazy from datetime import timezone
1 + lazy from datetime import timezone, UTC
2 |
  - timezone.utc  # snapshot: datetime-timezone-utc
3 + UTC  # snapshot: datetime-timezone-utc
  |
```

## Moving part of an import

`TC004` preserves `lazy` on both the imports moved to runtime and those remaining in the type-checking
block.

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    lazy import os, sys  # snapshot: runtime-import-in-type-checking-block
    lazy from pathlib import Path, PurePath  # snapshot: runtime-import-in-type-checking-block

print(os, Path)
```

```snapshot
error[TC004]: Move import `os` out of type-checking block. Import is used for more than type hinting.
 --> src/mdtest_snippet.py:4:17
  |
4 |     lazy import os, sys  # snapshot: runtime-import-in-type-checking-block
  |                 ^^
5 |     lazy from pathlib import Path, PurePath  # snapshot: runtime-import-in-type-checking-block
6 |
7 | print(os, Path)
  |       -- Used at runtime here
help: Move out of type-checking block
  |
1 | from typing import TYPE_CHECKING
2 + lazy import os
3 |
4 | if TYPE_CHECKING:
  -     lazy import os, sys  # snapshot: runtime-import-in-type-checking-block
5 +     lazy import sys  # snapshot: runtime-import-in-type-checking-block
6 |     lazy from pathlib import Path, PurePath  # snapshot: runtime-import-in-type-checking-block
  |
note: This is an unsafe fix and may change runtime behavior


error[TC004]: Move import `pathlib.Path` out of type-checking block. Import is used for more than type hinting.
 --> src/mdtest_snippet.py:5:30
  |
5 |     lazy from pathlib import Path, PurePath  # snapshot: runtime-import-in-type-checking-block
  |                              ^^^^
6 |
7 | print(os, Path)
  |           ---- Used at runtime here
help: Move out of type-checking block
  |
1 | from typing import TYPE_CHECKING
2 + lazy from pathlib import Path
3 |
4 | if TYPE_CHECKING:
5 |     lazy import os, sys  # snapshot: runtime-import-in-type-checking-block
  -     lazy from pathlib import Path, PurePath  # snapshot: runtime-import-in-type-checking-block
6 +     lazy from pathlib import PurePath  # snapshot: runtime-import-in-type-checking-block
7 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Replacing an import

`AIR321` preserves `lazy` when generating an import from the replacement module.

```py
lazy from airflow.utils.timezone import convert_to_utc

convert_to_utc  # snapshot: airflow31-moved
```

```snapshot
error[AIR321]: `airflow.utils.timezone.convert_to_utc` is moved in Airflow 3.1
 --> src/mdtest_snippet.py:3:1
  |
3 | convert_to_utc  # snapshot: airflow31-moved
  | ^^^^^^^^^^^^^^
help: `convert_to_utc` has been moved to `airflow.sdk.timezone` since Airflow 3.1 (with apache-airflow-task-sdk>=1.1.0).
  |
  - lazy from airflow.utils.timezone import convert_to_utc
1 + lazy from airflow.sdk.timezone import convert_to_utc
2 |
  |
note: This is an unsafe fix and may change runtime behavior
```
