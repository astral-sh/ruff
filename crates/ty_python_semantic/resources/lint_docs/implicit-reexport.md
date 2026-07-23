## What it does

Checks for imports and attribute accesses that use names from a runtime module
which imports but does not explicitly re-export them.

## Why is this bad?

Imports may only be intended for local use, not for re-export as part of the
public interface of a module. Importing names that were not intended for
re-export can create inconsistent import locations for the same symbol, cause
breakages when an internal import is removed, and make refactors more difficult.

## Rule status

This rule is disabled by default. Enable it to require runtime modules to use
explicit re-export conventions, matching the behavior that is always enforced
for stub files.

## Example

`models.py`:

```python
class Admin: ...


class User: ...
```

`api.py`:

```python
from models import Admin as Admin
from models import User
```

`app.py`:

```python
from api import Admin  # fine
from api import User  # error: [implicit-reexport]
```

The redundant alias marks `Admin` as part of the public interface of `api.py`.
Names can also be explicitly re-exported by including them in `__all__`.

## References

- [Typing specification: Import conventions](https://typing.python.org/en/latest/spec/distributing.html#import-conventions)
