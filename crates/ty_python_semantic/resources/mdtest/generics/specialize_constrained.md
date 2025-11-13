# Creating a specialization from a constraint set

```toml
[environment]
python-version = "3.12"
```

We create constraint sets to describe which types a set of typevars can specialize to. We have a
`specialize_constrained` method that creates a "best" specialization for a constraint set, which
lets us test this logic in isolation, without having to bring in the rest of the specialization
inference logic.

## Unbounded typevars

An unbounded typevar can specialize to any type. We will specialize the typevar to the least upper
bound of all of the types that satisfy the constraint set.

```py
from typing import Never
from ty_extensions import ConstraintSet, generic_context

def unbounded[T]():
    # revealed: ty_extensions.Specialization[T@unbounded = object]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.always()))

    # revealed: ty_extensions.Specialization[T@unbounded = int]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, int)))
    # revealed: ty_extensions.Specialization[T@unbounded = int]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(bool, T, int)))
```
