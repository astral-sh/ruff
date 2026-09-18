# `deprecated-c-element-tree` (`UP023`)

```toml
target-version = "py315"

[lint]
select = ["UP023"]
```

## Only the deprecated module listed

When only `cElementTree` is listed, replacing it with `ElementTree` would make the import eager, so
the diagnostic has no fix.

```py
__lazy_modules__ = ["xml.etree.cElementTree"]
import xml.etree.cElementTree as ET  # snapshot: deprecated-c-element-tree
```

```snapshot
error[UP023]: `cElementTree` is deprecated, use `ElementTree`
 --> src/mdtest_snippet.py:2:8
  |
2 | import xml.etree.cElementTree as ET  # snapshot: deprecated-c-element-tree
  |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `ElementTree`
```

## Members of the same containing module

Replacing a member imported from `xml.etree` leaves the containing module unchanged. The import
remains eager, so the diagnostic includes a fix.

```py
__lazy_modules__ = ["xml.etree.cElementTree"]
from xml.etree import cElementTree as ET  # snapshot: deprecated-c-element-tree
```

```snapshot
error[UP023]: `cElementTree` is deprecated, use `ElementTree`
 --> src/mdtest_snippet.py:2:23
  |
2 | from xml.etree import cElementTree as ET  # snapshot: deprecated-c-element-tree
  |                       ^^^^^^^^^^^^^^^^^^
help: Replace with `ElementTree`
  |
1 | __lazy_modules__ = ["xml.etree.cElementTree"]
  - from xml.etree import cElementTree as ET  # snapshot: deprecated-c-element-tree
2 + from xml.etree import ElementTree as ET  # snapshot: deprecated-c-element-tree
  |
```

## Star imports

Star imports remain eager regardless of `__lazy_modules__`, so replacing their module preserves
laziness and the diagnostic includes a fix.

```py
__lazy_modules__ = ["xml.etree.cElementTree"]
from xml.etree.cElementTree import *  # snapshot: deprecated-c-element-tree
```

```snapshot
error[UP023]: `cElementTree` is deprecated, use `ElementTree`
 --> src/mdtest_snippet.py:2:1
  |
2 | from xml.etree.cElementTree import *  # snapshot: deprecated-c-element-tree
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `ElementTree`
  |
1 | __lazy_modules__ = ["xml.etree.cElementTree"]
  - from xml.etree.cElementTree import *  # snapshot: deprecated-c-element-tree
2 + from xml.etree.ElementTree import *  # snapshot: deprecated-c-element-tree
  |
```
