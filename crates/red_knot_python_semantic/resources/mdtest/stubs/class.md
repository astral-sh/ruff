# Class definitions in stubs

## Cyclical class definition

In type stubs, classes can reference themselves in their base class definitions. For example, in
`typeshed`, we have `class str(Sequence[str]): ...`.

```pyi
class Foo[T]: ...

# TODO: actually is subscriptable
# error: [non-subscriptable]
class Bar(Foo[Bar]): ...

reveal_type(Bar)  # revealed: Literal[Bar]
reveal_type(Bar.__mro__)  # revealed: tuple[Literal[Bar], Unknown, Literal[object]]
```

## Access to attributes declarated in stubs

Unlike regular Python modules, stub files often omit the RHS in declarations, including in class
scope. However, from the perspective of the type checker, we have to treat them as bindings too.
That is, `symbol: type` is the same as `symbol: type = ...`.

One implication of this is that we'll always treat symbols in class scope as either
`class or instance` or `class only` (if `ClassVar` is used). We'll never infer a pure instance
attribute from a stub.

`b.pyi`:

```pyi
from typing import ClassVar

class C:
    class_or_instance_var: int
```

```py
from typing import ClassVar, Literal

from b import C

# No error here, since we treat `class_or_instance_var` as bound on the class.
reveal_type(C.class_or_instance_var)  # revealed: int
```
