# `typing.Final`

[`typing.Final`] is a type qualifier that is used to indicate that a symbol may not be reassigned in
any scope. Final names declared in class scopes cannot be overridden in subclasses.

## Basic type inference

### `Final` with type

Declared symbols that are additionally qualified with `Final` use the declared type when accessed
from another scope. Local uses of the symbol will use the inferred type, which may be more specific:

`mod.py`:

```py
from typing import Final, Annotated

FINAL_A: Final[int] = 1
FINAL_B: Annotated[Final[int], "the annotation for FINAL_B"] = 1
FINAL_C: Final[Annotated[int, "the annotation for FINAL_C"]] = 1
FINAL_D: "Final[int]" = 1
FINAL_F: Final[int]
FINAL_F = 1

reveal_type(FINAL_A)  # revealed: Literal[1]
reveal_type(FINAL_B)  # revealed: Literal[1]
reveal_type(FINAL_C)  # revealed: Literal[1]
reveal_type(FINAL_D)  # revealed: Literal[1]
reveal_type(FINAL_D)  # revealed: Literal[1]

def nonlocal_uses():
    reveal_type(FINAL_A)  # revealed: int
    reveal_type(FINAL_B)  # revealed: int
    reveal_type(FINAL_C)  # revealed: int
    reveal_type(FINAL_D)  # revealed: int
    reveal_type(FINAL_F)  # revealed: int
```

Imported types:

```py
from mod import FINAL_A, FINAL_B, FINAL_C, FINAL_D, FINAL_F

reveal_type(FINAL_A)  # revealed: int
reveal_type(FINAL_B)  # revealed: int
reveal_type(FINAL_C)  # revealed: int
reveal_type(FINAL_D)  # revealed: int
reveal_type(FINAL_F)  # revealed: int
```

### Bare `Final` without a type

When a symbol is qualified with `Final` but no type is specified, the type is inferred from the
right-hand side of the assignment. We do not union the inferred type with `Unknown`, because the
symbol cannot be modified:

`mod.py`:

```py
from typing import Final

FINAL_A: Final = 1

reveal_type(FINAL_A)  # revealed: Literal[1]

def nonlocal_uses():
    reveal_type(FINAL_A)  # revealed: Literal[1]
```

`main.py`:

```py
from mod import FINAL_A

reveal_type(FINAL_A)  # revealed: Literal[1]
```

### In class definitions

```py
from typing import Final

class C:
    FINAL_A: Final[int] = 1
    FINAL_B: Final = 1

    def __init__(self):
        self.FINAL_C: Final[int] = 1
        self.FINAL_D: Final = 1
        self.FINAL_E: Final
        self.FINAL_E = 1

reveal_type(C.FINAL_A)  # revealed: int
reveal_type(C.FINAL_B)  # revealed: Literal[1]

reveal_type(C().FINAL_A)  # revealed: int
reveal_type(C().FINAL_B)  # revealed: Literal[1]
reveal_type(C().FINAL_C)  # revealed: int
reveal_type(C().FINAL_D)  # revealed: Literal[1]
reveal_type(C().FINAL_E)  # revealed: Literal[1]
```

## Not modifiable

### Names

Symbols qualified with `Final` cannot be reassigned, and attempting to do so will result in an
error:

`mod.py`:

```py
from typing import Final, Annotated

FINAL_A: Final[int] = 1
FINAL_B: Annotated[Final[int], "the annotation for FINAL_B"] = 1
FINAL_C: Final[Annotated[int, "the annotation for FINAL_C"]] = 1
FINAL_D: "Final[int]" = 1
FINAL_E: Final[int]
FINAL_E = 1
FINAL_F: Final = 1

FINAL_A = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_A` is not allowed"
FINAL_B = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_B` is not allowed"
FINAL_C = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_C` is not allowed"
FINAL_D = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_D` is not allowed"
FINAL_E = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_E` is not allowed"
FINAL_F = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_F` is not allowed"

def global_use():
    global FINAL_A, FINAL_B, FINAL_C, FINAL_D, FINAL_E, FINAL_F
    FINAL_A = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_A` is not allowed"
    FINAL_B = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_B` is not allowed"
    FINAL_C = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_C` is not allowed"
    FINAL_D = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_D` is not allowed"
    FINAL_E = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_E` is not allowed"
    FINAL_F = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_F` is not allowed"

def local_use():
    # These are not errors, because they refer to local variables
    FINAL_A = 2
    FINAL_B = 2
    FINAL_C = 2
    FINAL_D = 2
    FINAL_E = 2
    FINAL_F = 2

def nonlocal_use():
    X: Final[int] = 1
    def inner():
        nonlocal X
        X = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `X` is not allowed: Reassignment of `Final` symbol"
```

`main.py`:

```py
from mod import FINAL_A, FINAL_B, FINAL_C, FINAL_D, FINAL_E, FINAL_F

FINAL_A = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_A` is not allowed"
FINAL_B = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_B` is not allowed"
FINAL_C = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_C` is not allowed"
FINAL_D = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_D` is not allowed"
FINAL_E = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_E` is not allowed"
FINAL_F = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `FINAL_F` is not allowed"
```

### Attributes

Assignments to attributes qualified with `Final` are also not allowed:

```py
from typing import Final

class Meta(type):
    META_FINAL_A: Final[int] = 1
    META_FINAL_B: Final = 1

class C(metaclass=Meta):
    CLASS_FINAL_A: Final[int] = 1
    CLASS_FINAL_B: Final = 1

    def __init__(self):
        self.INSTANCE_FINAL_A: Final[int] = 1
        self.INSTANCE_FINAL_B: Final = 1
        self.INSTANCE_FINAL_C: Final[int]
        self.INSTANCE_FINAL_C = 1

# error: [invalid-assignment] "Cannot assign to final attribute `META_FINAL_A` on type `<class 'C'>`"
C.META_FINAL_A = 2
# error: [invalid-assignment] "Cannot assign to final attribute `META_FINAL_B` on type `<class 'C'>`"
C.META_FINAL_B = 2

# error: [invalid-assignment] "Cannot assign to final attribute `CLASS_FINAL_A` on type `<class 'C'>`"
C.CLASS_FINAL_A = 2
# error: [invalid-assignment] "Cannot assign to final attribute `CLASS_FINAL_B` on type `<class 'C'>`"
C.CLASS_FINAL_B = 2

c = C()
# error: [invalid-assignment] "Cannot assign to final attribute `CLASS_FINAL_A` on type `C`"
c.CLASS_FINAL_A = 2
# error: [invalid-assignment] "Cannot assign to final attribute `CLASS_FINAL_B` on type `C`"
c.CLASS_FINAL_B = 2
# error: [invalid-assignment] "Cannot assign to final attribute `INSTANCE_FINAL_A` on type `C`"
c.INSTANCE_FINAL_A = 2
# error: [invalid-assignment] "Cannot assign to final attribute `INSTANCE_FINAL_B` on type `C`"
c.INSTANCE_FINAL_B = 2
# error: [invalid-assignment] "Cannot assign to final attribute `INSTANCE_FINAL_C` on type `C`"
c.INSTANCE_FINAL_C = 2
```

## Mutability

Objects qualified with `Final` *can be modified*. `Final` represents a constant reference to an
object, but that object itself may still be mutable:

```py
from typing import Final

class C:
    x: int = 1

FINAL_C_INSTANCE: Final[C] = C()
FINAL_C_INSTANCE.x = 2

FINAL_LIST: Final[list[int]] = [1, 2, 3]
FINAL_LIST[0] = 4
```

## Overriding in subclasses

When a symbol is qualified with `Final` in a class, it cannot be overridden in subclasses.

```py
from typing import Final

class Base:
    FINAL_A: Final[int] = 1
    FINAL_B: Final[int] = 1
    FINAL_C: Final = 1

class Derived(Base):
    # TODO: This should be an error
    FINAL_A = 2
    # TODO: This should be an error
    FINAL_B: Final[int] = 2
    # TODO: This should be an error
    FINAL_C = 2
```

## Syntax and usage

### Legal syntactical positions

Final may only be used in assignments or variable annotations. Using it in any other position is an
error.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Final, ClassVar, Annotated
from ty_extensions import reveal_mro

LEGAL_A: Final[int] = 1
LEGAL_B: Final = 1
LEGAL_C: Final[int]
LEGAL_C = 1
LEGAL_D: Final
LEGAL_D = 1

class C:
    LEGAL_E: ClassVar[Final[int]] = 1
    LEGAL_F: Final[ClassVar[int]] = 1
    LEGAL_G: Annotated[Final[ClassVar[int]], "metadata"] = 1

    def __init__(self):
        self.LEGAL_H: Final[int] = 1
        self.LEGAL_I: Final[int]
        self.LEGAL_I = 1

# error: [invalid-type-form] "`Final` is not allowed in function parameter annotations"
def f(ILLEGAL: Final[int]) -> None:
    pass

# error: [invalid-type-form] "`Final` is not allowed in function parameter annotations"
def f[T](ILLEGAL: Final[T]) -> T:
    return ILLEGAL

# error: [invalid-type-form] "`Final` is not allowed in function return type annotations"
def f() -> Final[None]: ...

# error: [invalid-type-form] "`Final` is not allowed in function return type annotations"
def f[T](x: T) -> Final[T]:
    return x

# TODO: This should be an error
class Foo(Final[tuple[int]]): ...

# TODO: Show `Unknown` instead of `@Todo` type in the MRO; or ignore `Final` and show the MRO as if `Final` was not there
# revealed: (<class 'Foo'>, @Todo(Inference of subscript on special form), <class 'object'>)
reveal_mro(Foo)
```

### Attribute assignment outside `__init__`

Qualifying an instance attribute with `Final` outside of `__init__` is not allowed. The instance
attribute must be assigned only once, when the instance is created.

```py
from typing import Final

class C:
    def some_method(self):
        # TODO: This should be an error
        self.x: Final[int] = 1
```

### `Final` in loops

Using `Final` in a loop is not allowed.

```py
from typing import Final

for _ in range(10):
    # TODO: This should be an error
    i: Final[int] = 1
```

### Too many arguments

```py
from typing import Final

class C:
    # error: [invalid-type-form] "Type qualifier `typing.Final` expected exactly 1 argument, got 2"
    x: Final[int, str] = 1
```

### Trailing comma creates a tuple

A trailing comma in a subscript creates a single-element tuple. We need to handle this gracefully
and emit a proper error rather than crashing (see
[ty#1793](https://github.com/astral-sh/ty/issues/1793)).

```py
from typing import Final

# error: [invalid-type-form] "Tuple literals are not allowed in this context in a type expression: Did you mean `tuple[()]`?"
x: Final[(),] = 42

# error: [invalid-assignment] "Reassignment of `Final` symbol `x` is not allowed"
x = 56
```

### Illegal `Final` in type expression

```py
from typing import Final

# error: [invalid-type-form] "Type qualifier `typing.Final` is not allowed in type expressions (only in annotation expressions)"
x: list[Final[int]] = []  # Error!

class C:
    # error: [invalid-type-form]
    x: Final | int

    # error: [invalid-type-form]
    y: int | Final[str]
```

## No assignment

Some type checkers do not support a separate declaration and assignment for `Final` symbols, but
it's possible to support this in ty, and is useful for code that declares symbols `Final` inside
`if TYPE_CHECKING` blocks.

### Basic

```py
from typing import Final

DECLARED_THEN_BOUND: Final[int]
DECLARED_THEN_BOUND = 1
```

### No assignment

```py
from typing import Final

# TODO: This should be an error
NO_ASSIGNMENT_A: Final
# TODO: This should be an error
NO_ASSIGNMENT_B: Final[int]

class C:
    # TODO: This should be an error
    NO_ASSIGNMENT_A: Final
    # TODO: This should be an error
    NO_ASSIGNMENT_B: Final[int]

    DEFINED_IN_INIT: Final[int]

    def __init__(self):
        self.DEFINED_IN_INIT = 1
```

## Final attributes with Self annotation in `__init__`

Issue #1409: Final instance attributes should be assignable in `__init__` even when using `Self`
type annotation.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Final, Self

class ClassA:
    ID4: Final[int]  # OK because initialized in __init__

    def __init__(self: Self):
        self.ID4 = 1  # Should be OK

    def other_method(self: Self):
        # error: [invalid-assignment] "Cannot assign to final attribute `ID4` on type `Self@other_method`"
        self.ID4 = 2  # Should still error outside __init__

class ClassB:
    ID5: Final[int]

    def __init__(self):  # Without Self annotation
        self.ID5 = 1  # Should also be OK

reveal_type(ClassA().ID4)  # revealed: int
reveal_type(ClassB().ID5)  # revealed: int
```

## Reassignment to Final in `__init__`

Per PEP 591 and the typing conformance suite, Final attributes can be assigned in `__init__`.
Multiple assignments within `__init__` are allowed (matching mypy and pyright behavior). However,
assignment in `__init__` is not allowed if the attribute already has a value at class level.

```py
from typing import Final

# Case 1: Declared in class, assigned once in __init__ - ALLOWED
class DeclaredAssignedInInit:
    attr1: Final[int]

    def __init__(self):
        self.attr1 = 1  # OK: First and only assignment

# Case 2: Declared and assigned in class body - ALLOWED (no __init__ assignment)
class DeclaredAndAssignedInClass:
    attr2: Final[int] = 10

# Case 3: Reassignment when already assigned in class body
class ReassignmentFromClass:
    attr3: Final[int] = 10

    def __init__(self):
        # error: [invalid-assignment]
        self.attr3 = 20  # Error: already assigned in class body

# Case 4: Multiple assignments within __init__ itself
# Per conformance suite and PEP 591, all assignments in __init__ are allowed
class MultipleAssignmentsInInit:
    attr4: Final[int]

    def __init__(self):
        self.attr4 = 1  # OK: Assignment in __init__
        self.attr4 = 2  # OK: Multiple assignments in __init__ are allowed

class ConditionalAssignment:
    X: Final[int]

    def __init__(self, cond: bool):
        if cond:
            self.X = 42  # OK: Assignment in __init__
        else:
            self.X = 56  # OK: Multiple assignments in __init__ are allowed

# Case 5: Declaration and assignment in __init__ - ALLOWED
class DeclareAndAssignInInit:
    def __init__(self):
        self.attr5: Final[int] = 1  # OK: Declare and assign in __init__

# Case 6: Assignment outside __init__ should still fail
class AssignmentOutsideInit:
    attr6: Final[int]

    def other_method(self):
        # error: [invalid-assignment] "Cannot assign to final attribute `attr6`"
        self.attr6 = 1  # Error: Not in __init__
```

## Final assignment restrictions in `__init__`

`__init__` can only assign Final attributes on the class it's defining, and only to the first
parameter (`self`).

```py
from typing import Final

class C:
    x: Final[int] = 100

# Assignment from standalone function (even named __init__)
def _(c: C):
    # error: [invalid-assignment] "Cannot assign to final attribute `x`"
    c.x = 1  # Error: Not in C.__init__

def __init__(c: C):
    # error: [invalid-assignment] "Cannot assign to final attribute `x`"
    c.x = 1  # Error: Not a method

# Assignment from another class's __init__
class A:
    def __init__(self, c: C):
        # error: [invalid-assignment] "Cannot assign to final attribute `x`"
        c.x = 1  # Error: Not C's __init__

# Assignment to non-self parameter in __init__
class D:
    y: Final[int]

    def __init__(self, other: "D"):
        self.y = 1  # OK: Assigning to self
        # TODO: Should error - assigning to non-self parameter
        # Requires tracking which parameter the base expression refers to
        other.y = 2
```

## Full diagnostics

<!-- snapshot-diagnostics -->

Annotated assignment:

```py
from typing import Final

MY_CONSTANT: Final[int] = 1

# more code

MY_CONSTANT = 2  # error: [invalid-assignment]
```

Imported `Final` symbol:

```py
from _stat import ST_INO

ST_INO = 1  # error: [invalid-assignment]
```

[`typing.final`]: https://docs.python.org/3/library/typing.html#typing.Final
