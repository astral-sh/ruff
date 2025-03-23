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
# TODO: Instead of `Literal[Foo]`, we might eventually want to show a type that involves the type parameter.
reveal_type(Bar.__mro__)  # revealed: tuple[Literal[Bar], Literal[Foo], Literal[object]]
```

## Access to attributes declared in stubs

Unlike regular Python modules, stub files often omit the right-hand side in declarations, including
in class scope. However, from the perspective of the type checker, we have to treat them as bindings
too. That is, `symbol: type` is the same as `symbol: type = ...`.

One implication of this is that we'll always treat symbols in class scope as safe to be accessed
from the class object itself. We'll never infer a "pure instance attribute" from a stub.

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
