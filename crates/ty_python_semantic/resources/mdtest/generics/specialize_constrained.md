# Creating a specialization from a constraint set

```toml
[environment]
python-version = "3.12"
```

## initial tests

```py
from ty_extensions import ConstraintSet, generic_context

def f[T]():
    # revealed: ty_extensions.Specialization[T@f = object]
    reveal_type(generic_context(f).specialize_constrained(ConstraintSet.always()))
```
