# Unresolved import diagnostics

<!-- snapshot-diagnostics -->

## Using `from` with an unresolvable module

This example demonstrates the diagnostic when a `from` style import is used with a module that could
not be found:

```py
from does_not_exist import add  # error: [unresolved-import]

stat = add(10, 15)
```

## Using `from` with too many leading dots

This example demonstrates the diagnostic when a `from` style import is used with a presumptively
valid path, but where there are too many leading dots.

`package/__init__.py`:

```py
```

`package/foo.py`:

```py
def add(x, y):
    return x + y
```

`package/subpackage/subsubpackage/__init__.py`:

```py
from ....foo import add  # error: [unresolved-import]

stat = add(10, 15)
```

## Using `from` with an unknown current module

This is another case handled separately in Red Knot, where a `.` provokes relative module name
resolution, but where the module name is not resolvable.

```py
from .does_not_exist import add  # error: [unresolved-import]

stat = add(10, 15)
```

## Using `from` with an unknown nested module

Like the previous test, but with sub-modules to ensure the span is correct.

```py
from .does_not_exist.foo.bar import add  # error: [unresolved-import]

stat = add(10, 15)
```

## Using `from` with a resolvable module but unresolvable item

This ensures that diagnostics for an unresolvable item inside a resolvable import highlight the item
and not the entire `from ... import ...` statement.

`a.py`:

```py
does_exist1 = 1
does_exist2 = 2
```

```py
from a import does_exist1, does_not_exist, does_exist2  # error: [unresolved-import]
```

## An unresolvable import that does not use `from`

This ensures that an unresolvable `import ...` statement highlights just the module name and not the
entire statement.

```py
import does_not_exist  # error: [unresolved-import]

x = does_not_exist.foo
```
