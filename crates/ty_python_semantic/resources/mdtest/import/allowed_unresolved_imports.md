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

A literal pattern only matches the exact module name. ty continues to emit a diagnostic for
unresolved submodule imports:

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

But it doesn't match a module that only contains a component named `foo`, if that module's *first*
component is not `foo`:

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

But it doesn't match a module that only contains a component named `foo`, if that module's *last*
component is not `foo`:

`main.py`:

```py
from foo.bar import foo  # error: [unresolved-import]
```

### Middle `**`

`foo.**.bar` matches any module name where the first component is `foo` and the final component is
`bar`, including `foo.bar`, `foo.bar.baz.bar`, etc.

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

## `*` combined with `**`

### First component ends with `*`

This pattern matches "any module where the first component starts with `aws`":

```toml
[analysis]
allowed-unresolved-imports = ["aws*.**"]
```

```py
import aws
import awscli
import awscli.alias
import awscli.customizations.sagemaker
from awscli.customizations import sagemaker
import awws  # error: [unresolved-import]
import foo.aws  # error: [unresolved-import]
import caws  # error: [unresolved-import]

reveal_type(aws)  # revealed: Unknown
reveal_type(awscli)  # revealed: Unknown
reveal_type(awscli.alias)  # revealed: Unknown
reveal_type(awscli.customizations.sagemaker)  # revealed: Unknown
reveal_type(sagemaker)  # revealed: Unknown
reveal_type(awws)  # revealed: Unknown
reveal_type(foo)  # revealed: Unknown
reveal_type(foo.aws)  # revealed: Unknown
reveal_type(caws)  # revealed: Unknown
```

### First component has multiple `*`s

This pattern matches "any module where the first component contains the string `aws`":

```toml
[analysis]
allowed-unresolved-imports = ["*aws*.**"]
```

```py
import aws
import awscli
import awscli.alias
import awscli.customizations.sagemaker
import caws
import caws.foo.bar
from awscli.customizations import sagemaker
import awws  # error: [unresolved-import]
import foo.aws  # error: [unresolved-import]

reveal_type(aws)  # revealed: Unknown
reveal_type(awscli)  # revealed: Unknown
reveal_type(awscli.alias)  # revealed: Unknown
reveal_type(awscli.customizations.sagemaker)  # revealed: Unknown
reveal_type(sagemaker)  # revealed: Unknown
reveal_type(awws)  # revealed: Unknown
reveal_type(foo)  # revealed: Unknown
reveal_type(foo.aws)  # revealed: Unknown
reveal_type(caws)  # revealed: Unknown
reveal_type(caws.foo.bar)  # revealed: Unknown
```

## Negative patterns

Patterns can be negated, similar to gitignore. If multiple patterns match a given import, a later
matching pattern (positive or negated) will always have priority over an earlier one in the list. If
the last matching pattern is positive, a module will be allowlisted. If the last matching pattern is
negated, ty's usual `unresolved-import` diagnostics will be emitted for the import.

### Negative pattern after positive pattern

This pattern excludes all modules where the first component is `test`, *except* `test.foo`:

```toml
[analysis]
allowed-unresolved-imports = ["test.**", "!test.foo"]
```

```py
from test.bar import baz
from test.foo import bar  # error: [unresolved-import]
```

This syntax can be useful when overriding parent configurations. For example, a project's main
configuration might allowlist all `test`-module imports, but an overriding configuration in a
subproject could use a negated pattern to reapply strict `unresolved-import` settings for `test.foo`
modules specifically.

### Positive pattern after negative pattern

This configuration indicates that only the `test.foo` import should be allowed to be unresolved
without ty emitting a diagnostic:

```toml
[analysis]
allowed-unresolved-imports = ["test.**", "!test.**", "test.foo"]
```

```py
from test.bar import baz  # error: [unresolved-import]
from test.foo import bar
```

## Missing module member

When importing a member from a module that exists but doesn't have that member, ty would normally
emit "Module `X` has no member `Y`". Setting `allow-unresolved-imports` can suppress this diagnostic
also:

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
