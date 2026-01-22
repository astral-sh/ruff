# Allowed unresolved imports

ty should not emit a diagnostic for unresolved modules matching a pattern in
`allowed-unresolved-imports`.

## Literal matches

```toml
[analysis]
allowed-unresolved-imports = ["foo"]
```

```py
from foo import bar
import foo

reveal_type(foo)  # revealed: Unknown
reveal_type(bar)  # revealed: Unknown
```

A literal pattern only matches the exact module name, ty continues to emit a diagnostic for
unresolved submodule imports

```py
from foo.sub import bar  # error: [unresolved-import]
import foo.sub.bar  # error: [unresolved-import]
```

## `**` wildcard matches

`**` matches zero to any number of components.

### Ending `**`

`foo.**` matches any module name that starts with `foo`, including `foo`, `foo.bar`, `foo.bar.baz`,
etc.

```toml
[analysis]
allowed-unresolved-imports = ["foo.**"]
```

```py
from foo import bar
from foo.sub import bar2
from foo.sub.baz import bar3
import foo.baz
```

But it doesn't match any module that only contains a component named `foo`

```py
from bar import foo  # error: [unresolved-import]
```

### Starting `**`

`**.foo` matches any module name that ends with `foo`, including `foo`, `bar.foo`, `baz.bar.foo`,
etc.

```toml
[analysis]
allowed-unresolved-imports = ["**.foo"]
```

```py
from foo import bar
from bar.foo import baz
from baz.bar.foo import qux
import bar.foo
```

But it doesn't match any module that only contains a component named `foo`

`main.py`:

```py
from foo.bar import foo  # error: [unresolved-import]
```

### Middle `**`

`foo.**.bar` matches any module name starting with `foo` and ending with `bar`, including `foo.bar`,
`foo.bar.baz.bar`, etc.

```toml
[analysis]
allowed-unresolved-imports = ["foo.**.bar"]
```

```py
from foo.bar import baz
from foo.bar.baz.bar import qux
import foo.bar.baz.bar
```

## `*` wildcard matches

`*` matches zero or more characters, but not `.`.

```toml
[analysis]
allowed-unresolved-imports = ["test*.foo"]
```

```py
from test.foo import bar
from testing.foo import baz
```

it doesn't match `.`

```py
import test.ing.foo  # error: [unresolved-import]
```

## Negative patterns

Patterns can be negated, similar to gitignore. Patterns are matched from the end to start and a
module is allowlisted if the first patching pattern is not negated and denied otherwise.

### Negative pattern after positive pattern

This pattern excludes all `test` modules except `test.foo`

```toml
[analysis]
allowed-unresolved-imports = ["test.**", "!test.foo"]
```

```py
from test.bar import baz
from test.foo import bar  # error: [unresolved-import]
```

Only allowlist `test.foo` but not any other `test` module. This syntax can be useful when overriding
configurations where the main configuration allowlists all `test` modules but the override then
denies all `test` modules except `test.foo`.

### Positive pattern after negative pattern

This pattern allows all `test` modules except `test.foo`

```toml
[analysis]
allowed-unresolved-imports = ["test.**", "!test.**", "test.foo"]
```

```py
from test.bar import baz  # error: [unresolved-import]
from test.foo import bar
```

## Missing module member

When importing a member from a module that exists but doesn't have that member, ty emits "Module `X`
has no member `Y`".

```toml
[analysis]
allowed-unresolved-imports = ["pkg.nonexistent"]
```

`pkg/__init__.py`:

```py
x = 1
```

`pkg/a.py`:

```py
from pkg import nonexistent
```
