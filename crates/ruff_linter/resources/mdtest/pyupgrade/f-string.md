# `f-string` (`UP032`)

```toml
lint.select = ["UP032"]
```

## Format strings with no equivalent f-string are skipped

Some `str.format` calls parse fine but have no f-string that behaves the same, so UP032 leaves them
alone.

A signed index raises `KeyError` (Python reads `+0` as a name), but an f-string would read it as
index `0`:

```py
"{+0}".format(0)
"{-0}".format(0)
```

A non-identifier attribute is `getattr(x, " a")`, whereas `f"{x. a}"` reads `x.a`:

```py
"{. a}".format(x)
```

A string key that contains the quote the f-string would wrap it in:

```py
"{[']}".format(x)
```

A conversion other than `s`, `r`, or `a` raises `ValueError`:

```py
"{!?}".format(0)
```

## Valid conversions are unaffected

```py
"{!r}".format(x)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:1:1
  |
1 | "{!r}".format(x)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
  - "{!r}".format(x)  # snapshot: f-string
1 + f"{x!r}"  # snapshot: f-string
```
