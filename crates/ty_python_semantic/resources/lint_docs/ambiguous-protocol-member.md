## What it does

Checks for protocol classes with members that will lead to ambiguous interfaces.

## Why is this bad?

Assigning to an undeclared variable in a protocol class, or to an undeclared attribute
through a protocol method's `self` or `cls` receiver, leads to an ambiguous interface
which may lead to the type checker inferring unexpected things. It's recommended to
ensure that all members of a protocol class are explicitly declared.

## Examples

```py
from typing import ClassVar, Protocol


class BaseProto(Protocol):
    a: int  # fine (explicitly declared as `int`)
    instance_member: str
    class_member: ClassVar[str]

    # fine: a method definition using `def` is considered a declaration
    def method_member(self) -> int: ...

    def method(self) -> None:
        self.instance_member = "value"  # fine (declared in the class body)
        self.implicit = "value"  # error: [ambiguous-protocol-member]

    @classmethod
    def class_method(cls) -> None:
        cls.class_member = "value"  # fine (declared in the class body)
        cls.implicit_class = "value"  # error: [ambiguous-protocol-member]

    # no explicit declaration, leading to ambiguity
    c = "some variable"  # error
    # no explicit declaration, leading to ambiguity
    b = method_member  # error

    # This creates implicit assignments of `d` and `e` in the protocol class body.
    # Were they really meant to be considered protocol members?
    # error: "`d` is not declared as a protocol member"
    # error: "`e` is not declared as a protocol member"
    for d, e in enumerate(range(42)):
        pass


class SubProto(BaseProto, Protocol):
    a = 42  # fine (declared in superclass)
```
