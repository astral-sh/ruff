# `magic-value-comparison` (`PLR2004`)

```toml
[lint]
select = ["PLR2004"]
```

## `sys.version` family

Comparisons against `sys.version`, `sys.version_info`, and
`sys.implementation.version` are exempt, including subscripts and attribute
access on them. The literal in those comparisons names a specific Python
version, not an unnamed constant, so no diagnostics fire below:

```py
import sys

if sys.version_info[0] >= 3:
    pass

if sys.version_info.major >= 3:
    pass

if sys.implementation.version[0] >= 3:
    pass

if sys.implementation.version[0] < 3:
    pass

if 3 <= sys.implementation.version[0]:
    pass

if sys.implementation.version.major >= 3:
    pass

if sys.version[0] >= "3":
    pass

if sys.version >= "3.10":
    pass

if 0 < sys.implementation.version[0] < 4:
    pass
```

The exemption only applies when the magic value is compared *against* a
member of the `sys.version` family. An unrelated comparison with the same
literal still fires:

```py
x = 3

if x >= 3:  # error: [magic-value-comparison]
    pass
```
