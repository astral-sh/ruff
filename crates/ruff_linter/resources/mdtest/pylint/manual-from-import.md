# `manual-from-import` (`PLR0402`)

```toml
target-version = "py315"

[lint]
select = ["PLR0402"]
```

## Both modules listed

When `__lazy_modules__` contains both the package and its submodule, the fix preserves laziness.

```py
__lazy_modules__ = ["foo", "foo.bar"]
import foo.bar as bar  # snapshot: manual-from-import
```

```snapshot
error[PLR0402]: Use `from foo import bar` in lieu of alias
 --> src/mdtest_snippet.py:2:8
  |
2 | import foo.bar as bar  # snapshot: manual-from-import
  |        ^^^^^^^^^^^^^^
help: Replace with `from foo import bar`
  |
1 | __lazy_modules__ = ["foo", "foo.bar"]
  - import foo.bar as bar  # snapshot: manual-from-import
2 + from foo import bar  # snapshot: manual-from-import
  |
```

## Only the submodule listed

A `from` import checks its containing module for membership in `__lazy_modules__`. Rewriting this
import would make it eager, so the diagnostic has no fix.

```py
__lazy_modules__ = ["foo.bar"]
import foo.bar as bar  # snapshot: manual-from-import
```

```snapshot
error[PLR0402]: Use `from foo import bar` in lieu of alias
 --> src/mdtest_snippet.py:2:8
  |
2 | import foo.bar as bar  # snapshot: manual-from-import
  |        ^^^^^^^^^^^^^^
help: Replace with `from foo import bar`
```

## Only the package listed

Listing a package does not make imports of its submodules lazy. Rewriting this import would make it
lazy, so the diagnostic has no fix.

```py
__lazy_modules__ = ["foo"]
import foo.bar as bar  # snapshot: manual-from-import
```

```snapshot
error[PLR0402]: Use `from foo import bar` in lieu of alias
 --> src/mdtest_snippet.py:2:8
  |
2 | import foo.bar as bar  # snapshot: manual-from-import
  |        ^^^^^^^^^^^^^^
help: Replace with `from foo import bar`
```

## Explicit lazy imports

The fix preserves the `lazy` keyword, so the import remains lazy even when the package is absent
from `__lazy_modules__`.

```py
__lazy_modules__ = ["foo.bar"]
lazy import foo.bar as bar  # snapshot: manual-from-import
```

```snapshot
error[PLR0402]: Use `from foo import bar` in lieu of alias
 --> src/mdtest_snippet.py:2:13
  |
2 | lazy import foo.bar as bar  # snapshot: manual-from-import
  |             ^^^^^^^^^^^^^^
help: Replace with `from foo import bar`
  |
1 | __lazy_modules__ = ["foo.bar"]
  - lazy import foo.bar as bar  # snapshot: manual-from-import
2 + lazy from foo import bar  # snapshot: manual-from-import
  |
```

## Unknown declarations

When the declaration's contents are unknown, the rule cannot establish whether the fix preserves
laziness, so no fix is offered.

```py
__lazy_modules__ = configured_lazy_modules
import foo.bar as bar  # snapshot: manual-from-import
```

```snapshot
error[PLR0402]: Use `from foo import bar` in lieu of alias
 --> src/mdtest_snippet.py:2:8
  |
2 | import foo.bar as bar  # snapshot: manual-from-import
  |        ^^^^^^^^^^^^^^
help: Replace with `from foo import bar`
```

## Imports inside functions

Imports inside functions are eager regardless of `__lazy_modules__`, so an unknown declaration does
not prevent a fix.

```py
__lazy_modules__ = configured_lazy_modules


def import_in_function():
    import foo.bar as bar  # snapshot: manual-from-import
```

```snapshot
error[PLR0402]: Use `from foo import bar` in lieu of alias
 --> src/mdtest_snippet.py:5:12
  |
5 |     import foo.bar as bar  # snapshot: manual-from-import
  |            ^^^^^^^^^^^^^^
help: Replace with `from foo import bar`
  |
4 | def import_in_function():
  -     import foo.bar as bar  # snapshot: manual-from-import
5 +     from foo import bar  # snapshot: manual-from-import
  |
```
