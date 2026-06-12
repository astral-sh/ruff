## What it does

Checks for class definitions where the metaclass of the class
being created would not be a subclass of the metaclasses of
all the class's bases.

## Why is it bad?

Such a class definition raises a `TypeError` at runtime.

## Examples

```pyi
class M1(type): ...
class M2(type): ...
class A(metaclass=M1): ...
class B(metaclass=M2): ...

# TypeError: metaclass conflict
class C(A, B): ...  # error
```
