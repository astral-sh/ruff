# `typing.Type`

## Annotation

`typing.Type` can be used interchangeably with `type`:

```py
from typing import Type

class A: ...

def _(c: Type, d: Type[A]):
    reveal_type(c)  # revealed: type
    reveal_type(d)  # revealed: type[A]
    c = d  # fine
    d = c  # fine
```

## Legacy generic aliases

Legacy generic aliases nested inside `type[...]` are not fully supported yet. They retain the same
fallback in evaluated and string annotations.

```py
from typing import Set, Tuple, Type

def f(a: Type[Set[int]], b: type[Tuple[int]], c: "type[Tuple[int]]"):
    reveal_type(a)  # revealed: @Todo(unsupported nested subscript in type[X])
    reveal_type(b)  # revealed: @Todo(unsupported nested subscript in type[X])
    reveal_type(c)  # revealed: @Todo(unsupported nested subscript in type[X])
```

## Invalid arguments in unsupported string annotations

Unsupported `type[...]` arguments are still checked as type expressions. Missing names and invalid
calls are reported instead of silently accepting the annotation.

`runtime.py`:

```py
from typing import Any, Tuple

# error: [unresolved-reference] "Name `missing_alias` used when not defined"
alias: "type[Tuple[missing_alias]]"
# error: [invalid-type-form] "Function calls are not allowed"
call: "type[missing_call()]"
# error: [unresolved-reference] "Name `missing_any` used when not defined"
any_annotation: "type[Any[missing_any]]"
```

Stub files retain the same diagnostics.

`stub.pyi`:

```pyi
from typing import Any, Tuple

# error: [unresolved-reference] "Name `missing_alias` used when not defined"
alias: "type[Tuple[missing_alias]]"
# error: [invalid-type-form] "Function calls are not allowed"
call: "type[missing_call()]"
# error: [unresolved-reference] "Name `missing_any` used when not defined"
any_annotation: "type[Any[missing_any]]"
```

## Inheritance

Inheriting from `Type` results in a MRO with `builtins.type` and `typing.Generic`. `Type` itself is
not a class.

```py
from typing import Type
from ty_extensions._internal import reveal_mro

class C(Type): ...

# Runtime value: `(C, type, typing.Generic, object)`
# TODO: Add `Generic` to the MRO
reveal_mro(C)  # revealed: (<class 'C'>, <class 'type'>, <class 'object'>)
```
