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
from typing import Any, Never
from ty_extensions import ConstraintSet, generic_context

# fmt: off

def unbounded[T]():
    # revealed: ty_extensions.Specialization[T@unbounded = Unknown]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.always()))
    # revealed: ty_extensions.Specialization[T@unbounded = object]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, object)))
    # revealed: ty_extensions.Specialization[T@unbounded = Any]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, Any)))
    # revealed: None
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@unbounded = int]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, int)))
    # revealed: ty_extensions.Specialization[T@unbounded = int]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(bool, T, int)))

    # revealed: ty_extensions.Specialization[T@unbounded = bool]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, int) & ConstraintSet.range(Never, T, bool)))
    # revealed: ty_extensions.Specialization[T@unbounded = Never]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, int) & ConstraintSet.range(Never, T, str)))
    # revealed: None
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(bool, T, bool) & ConstraintSet.range(Never, T, str)))

    # TODO: revealed: ty_extensions.Specialization[T@unbounded = int]
    # revealed: ty_extensions.Specialization[T@unbounded = bool]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, bool)))
    # revealed: ty_extensions.Specialization[T@unbounded = Never]
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(Never, T, int) | ConstraintSet.range(Never, T, str)))
    # revealed: None
    reveal_type(generic_context(unbounded).specialize_constrained(ConstraintSet.range(bool, T, bool) | ConstraintSet.range(Never, T, str)))
```

## Typevar with an upper bound

If a typevar has an upper bound, then it must specialize to a type that is a subtype of that bound.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, generic_context

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def bounded[T: Base]():
    # revealed: ty_extensions.Specialization[T@bounded = Base]
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.always()))
    # revealed: ty_extensions.Specialization[T@bounded = Base]
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.range(Never, T, object)))
    # revealed: ty_extensions.Specialization[T@bounded = Base & Any]
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.range(Never, T, Any)))
    # revealed: None
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@bounded = Base]
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.range(Never, T, Super)))
    # revealed: ty_extensions.Specialization[T@bounded = Base]
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.range(Never, T, Base)))
    # revealed: ty_extensions.Specialization[T@bounded = Sub]
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.range(Never, T, Sub)))

    # revealed: ty_extensions.Specialization[T@bounded = Never]
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.range(Never, T, Unrelated)))
    # revealed: None
    reveal_type(generic_context(bounded).specialize_constrained(ConstraintSet.range(Unrelated, T, Unrelated)))
```

If the upper bound is a gradual type, we are free to choose any materialization of the upper bound
that makes the test succeed.

```py
from typing import Any

def bounded_by_gradual[T: Any]():
    # TODO: revealed: ty_extensions.Specialization[T@bounded_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual = object]
    reveal_type(generic_context(bounded_by_gradual).specialize_constrained(ConstraintSet.always()))
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual = object]
    reveal_type(generic_context(bounded_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, object)))
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual = Any]
    reveal_type(generic_context(bounded_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, Any)))
    # revealed: None
    reveal_type(generic_context(bounded_by_gradual).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@bounded_by_gradual = Base]
    reveal_type(generic_context(bounded_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, Base)))
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual = object]
    reveal_type(generic_context(bounded_by_gradual).specialize_constrained(ConstraintSet.range(Base, T, object)))

    # revealed: ty_extensions.Specialization[T@bounded_by_gradual = Unrelated]
    reveal_type(generic_context(bounded_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, Unrelated)))

def bounded_by_gradual_list[T: list[Any]]():
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = Top[list[Any]]]
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.always()))
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = list[object]]
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[object])))
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = list[Any]]
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[Any])))
    # revealed: None
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = list[Base]]
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[Base])))
    # TODO: revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = list[Base]]
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = Top[list[Any]]]
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.range(list[Base], T, object)))

    # revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = list[Unrelated]]
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[Unrelated])))
    # TODO: revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = list[Unrelated]]
    # revealed: ty_extensions.Specialization[T@bounded_by_gradual_list = Top[list[Any]]]
    reveal_type(generic_context(bounded_by_gradual_list).specialize_constrained(ConstraintSet.range(list[Unrelated], T, object)))
```

## Constrained typevar

If a typevar has constraints, then it must specialize to one of those specific types. (Not to a
subtype of one of those types!)

In particular, note that if a constraint set is satisfied by more than one of the typevar's
constraints (i.e., we have no reason to prefer one over the others), then we return `None` to
indicate an ambiguous result. We could, in theory, return _more than one_ specialization, since we
have all of the information necessary to produce this. But it's not clear what we would do with that
information at the moment.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, generic_context

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

def constrained[T: (Base, Unrelated)]():
    # revealed: None
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.always()))
    # revealed: None
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Never, T, object)))
    # revealed: None
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Never, T, Any)))
    # revealed: None
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@constrained = Base]
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Never, T, Base)))
    # revealed: ty_extensions.Specialization[T@constrained = Base]
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Base, T, object)))

    # revealed: ty_extensions.Specialization[T@constrained = Unrelated]
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Never, T, Unrelated)))
    # revealed: ty_extensions.Specialization[T@constrained = Unrelated]
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Unrelated, T, object)))

    # revealed: ty_extensions.Specialization[T@constrained = Base]
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Never, T, Super)))
    # revealed: None
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Super, T, Super)))

    # revealed: ty_extensions.Specialization[T@constrained = Base]
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Sub, T, object)))
    # revealed: None
    reveal_type(generic_context(constrained).specialize_constrained(ConstraintSet.range(Sub, T, Sub)))
```

If any of the constraints is a gradual type, we are free to choose any materialization of that
constraint that makes the test succeed.

TODO: At the moment, we are producing a specialization that shows which particular materialization
that we chose, but really, we should be returning the gradual constraint as the specialization.

```py
from typing import Any

# fmt: off

def constrained_by_gradual[T: (Base, Any)]():
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Unknown]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Base]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.always()))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Base]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, object)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Base & Any]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, Any)))
    # revealed: None
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.never()))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Base]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, Base)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Base]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Base, T, object)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Unrelated]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, Unrelated)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = object]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Unrelated, T, object)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Base]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Never, T, Super)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Super]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Super, T, Super)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Base]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Sub, T, object)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual = Sub]
    reveal_type(generic_context(constrained_by_gradual).specialize_constrained(ConstraintSet.range(Sub, T, Sub)))

def constrained_by_two_gradual[T: (Any, Any)]():
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = object]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.always()))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = object]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Never, T, object)))
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = Any]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Never, T, Any)))
    # revealed: None
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.never()))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = Base]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Never, T, Base)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = Unrelated]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Never, T, Unrelated)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = Super]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Never, T, Super)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = Super]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Super, T, Super)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = object]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Sub, T, object)))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = Any]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual = Sub]
    reveal_type(generic_context(constrained_by_two_gradual).specialize_constrained(ConstraintSet.range(Sub, T, Sub)))

def constrained_by_gradual_list[T: (list[Base], list[Any])]():
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Base]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.always()))
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[object]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[object])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Base] & list[Any]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[Any])))
    # revealed: None
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Base]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[Base])))
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Base]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(list[Base], T, object)))

    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Unrelated]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[Unrelated])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Unrelated]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = Top[list[Any]]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(list[Unrelated], T, object)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Super]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(Never, T, list[Super])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Super]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(list[Super], T, list[Super])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list = list[Sub]]
    reveal_type(generic_context(constrained_by_gradual_list).specialize_constrained(ConstraintSet.range(list[Sub], T, list[Sub])))

# Same tests as above, but with the typevar constraints in a different order, to make sure the
# results do not depend on our BDD variable ordering.
def constrained_by_gradual_list_reverse[T: (list[Any], list[Base])]():
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Base]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.always()))
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[object]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(Never, T, list[object])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Base] & list[Any]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(Never, T, list[Any])))
    # revealed: None
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Base]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(Never, T, list[Base])))
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Base]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(list[Base], T, object)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Unrelated]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(Never, T, list[Unrelated])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Unrelated]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = Top[list[Any]]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(list[Unrelated], T, object)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Super]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(Never, T, list[Super])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Super]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(list[Super], T, list[Super])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_gradual_list_reverse = list[Sub]]
    reveal_type(generic_context(constrained_by_gradual_list_reverse).specialize_constrained(ConstraintSet.range(list[Sub], T, list[Sub])))

def constrained_by_two_gradual_lists[T: (list[Any], list[Any])]():
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = Top[list[Any]]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.always()))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = Top[list[Any]]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(Never, T, object)))
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = list[Any]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(Never, T, list[Any])))
    # revealed: None
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = list[Base]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(Never, T, list[Base])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Base]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = Top[list[Any]]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(list[Base], T, object)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = list[Unrelated]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(Never, T, list[Unrelated])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Unrelated]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = Top[list[Any]]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(list[Unrelated], T, object)))

    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = list[Super]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(Never, T, list[Super])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = list[Super]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(list[Super], T, list[Super])))
    # TODO: revealed: ty_extensions.Specialization[T@constrained_by_gradual = list[Any]]
    # revealed: ty_extensions.Specialization[T@constrained_by_two_gradual_lists = list[Sub]]
    reveal_type(generic_context(constrained_by_two_gradual_lists).specialize_constrained(ConstraintSet.range(list[Sub], T, list[Sub])))
```

## Mutually constrained typevars

If one typevar is constrained by another, the specialization of one can affect the specialization of
the other.

```py
from typing import final, Never
from ty_extensions import ConstraintSet, generic_context

class Super: ...
class Base(Super): ...
class Sub(Base): ...

@final
class Unrelated: ...

# fmt: off

def mutually_bound[T: Base, U]():
    # revealed: ty_extensions.Specialization[T@mutually_bound = Base, U@mutually_bound = Unknown]
    reveal_type(generic_context(mutually_bound).specialize_constrained(ConstraintSet.always()))
    # revealed: None
    reveal_type(generic_context(mutually_bound).specialize_constrained(ConstraintSet.never()))

    # revealed: ty_extensions.Specialization[T@mutually_bound = Base, U@mutually_bound = Base]
    reveal_type(generic_context(mutually_bound).specialize_constrained(ConstraintSet.range(Never, U, T)))

    # revealed: ty_extensions.Specialization[T@mutually_bound = Sub, U@mutually_bound = Unknown]
    reveal_type(generic_context(mutually_bound).specialize_constrained(ConstraintSet.range(Never, T, Sub)))
    # revealed: ty_extensions.Specialization[T@mutually_bound = Sub, U@mutually_bound = Sub]
    reveal_type(generic_context(mutually_bound).specialize_constrained(ConstraintSet.range(Never, T, Sub) & ConstraintSet.range(Never, U, T)))
    # revealed: ty_extensions.Specialization[T@mutually_bound = Base, U@mutually_bound = Sub]
    reveal_type(generic_context(mutually_bound).specialize_constrained(ConstraintSet.range(Never, U, Sub) & ConstraintSet.range(Never, U, T)))
```

## Nested typevars

A typevar's constraint can _mention_ another typevar without _constraining_ it. In this example, `U`
must be specialized to `list[T]`, but it cannot affect what `T` is specialized to.

```py
from typing import Never
from ty_extensions import ConstraintSet, generic_context

def mentions[T, U]():
    # (T@mentions ≤ int) ∧ (U@mentions = list[T@mentions])
    constraints = ConstraintSet.range(Never, T, int) & ConstraintSet.range(list[T], U, list[T])
    # TODO: revealed: ty_extensions.Specialization[T@mentions = int, U@mentions = list[int]]
    # revealed: ty_extensions.Specialization[T@mentions = int, U@mentions = Unknown]
    reveal_type(generic_context(mentions).specialize_constrained(constraints))
```

If the constraint set contains mutually recursive bounds, specialization inference will not
converge. This test ensures that our cycle detection prevents an endless loop or stack overflow in
this case.

```py
def divergent[T, U]():
    # (T@divergent = list[U@divergent]) ∧ (U@divergent = list[T@divergent]))
    constraints = ConstraintSet.range(list[U], T, list[U]) & ConstraintSet.range(list[T], U, list[T])
    # revealed: None
    reveal_type(generic_context(divergent).specialize_constrained(constraints))
```
