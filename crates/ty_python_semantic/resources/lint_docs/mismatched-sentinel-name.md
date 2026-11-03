## What it does

Checks for `builtins.sentinel` and `typing_extensions.Sentinel` definitions whose name argument does
not match the variable they are assigned to. In class scope, the name can be unqualified or include
the full enclosing class path.

## Why is this bad?

A mismatched name can be a typo and can make a sentinel's runtime representation confusing.

## Default level

This rule is disabled by default because sentinel names are not required to match their assigned
variables, and sometimes the preferred `__repr__` for a sentinel may include non-identifier
characters.

Enable this rule to enforce consistent sentinel naming.

This rule is similar to `mismatched-type-name`, but is a distinct rule because the default severity
differs.

## Examples

```python
from typing_extensions import Sentinel

MISSING = Sentinel("OTHER")  # error: [mismatched-sentinel-name]
```

Use a matching name instead:

```python
from typing_extensions import Sentinel

MISSING = Sentinel("MISSING")


class Container:
    UNSET = Sentinel("Container.UNSET")
```
