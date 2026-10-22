# `magic-value-comparison` (`PLR2004`)

```toml
[lint]
select = ["PLR2004"]
```

## Bitwise inversion

```py
x = 0
x == ~0  # error: [magic-value-comparison]
x == ~1  # error: [magic-value-comparison]
x == ~2  # error: [magic-value-comparison]
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

## Allowed values

```toml
[lint]
select = ["PLR2004"]

[lint.pylint]
allow-magic-value-types = []
allow-magic-values = ["allowed", -2.5, 2147483648, -2147483648]
extend-allow-magic-values = [3.5]
```

```py
x = 0
x == "allowed"  # no diagnostic
x == "other"  # error: [magic-value-comparison]
x == -2.5  # no diagnostic
x == +2.5  # error: [magic-value-comparison]
x == +3.5  # no diagnostic
x == 1  # error: [magic-value-comparison]
```

Large integers work too. Allowing `2147483648` doesn't also allow `2147483648.0`.

```py
x == 2147483648  # no diagnostic
x == -2147483648  # no diagnostic
x == +2147483648  # no diagnostic
x == ~2147483648  # error: [magic-value-comparison]
x == 2147483649  # error: [magic-value-comparison]
x == -2147483649  # error: [magic-value-comparison]
x == 2147483648.0  # error: [magic-value-comparison]
```
