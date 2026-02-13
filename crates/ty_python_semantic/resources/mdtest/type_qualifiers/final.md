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

### Reassignment after conditional assignment

If a `Final` symbol is conditionally assigned, a subsequent unconditional assignment is still a
reassignment error, because the symbol may have already been bound:

```py
from typing import Final

def cond() -> bool:
    return True

ABC: Final[int]

if cond():
    ABC = 1

ABC = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `ABC` is not allowed"
```

Assigning in both branches of an `if/else` is fine — each branch is a first assignment:

```py
from typing import Final

def cond() -> bool:
    return True

X: Final[int]

if cond():
    X = 1
else:
    X = 2
```

But assigning in both branches and then again unconditionally is an error:

```py
from typing import Final

def cond() -> bool:
    return True

Y: Final[int]

if cond():
    Y = 1
else:
    Y = 2

Y = 3  # error: [invalid-assignment] "Reassignment of `Final` symbol `Y` is not allowed"
```

Multiple conditional blocks don't help — the second `if` body sees that the first may have already
bound the symbol:

```py
from typing import Final

def cond() -> bool:
    return True

Z: Final[int]

if cond():
    Z = 1

if cond():
    Z = 2  # error: [invalid-assignment] "Reassignment of `Final` symbol `Z` is not allowed"
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

### Basic override detection

```py
from typing import Final

class Base:
    FINAL_A: Final[int] = 1
    FINAL_B: Final[int] = 1
    FINAL_C: Final = 1

class Derived(Base):
    # error: [override-of-final-variable] "Cannot override final variable `FINAL_A` from superclass `Base`"
    FINAL_A = 2
    # error: [override-of-final-variable] "Cannot override final variable `FINAL_B` from superclass `Base`"
    FINAL_B: Final[int] = 2
    # error: [override-of-final-variable] "Cannot override final variable `FINAL_C` from superclass `Base`"
    FINAL_C = 2
```

### Transitive override through MRO

Overriding a `Final` variable is also detected transitively through the MRO:

```py
from typing import Final

class GrandBase:
    X: Final[int] = 1

class Parent(GrandBase): ...

class Child(Parent):
    X = 2  # error: [override-of-final-variable] "Cannot override final variable `X` from superclass `GrandBase`"
```

### Non-`Final` variables are unaffected

Non-`Final` class variables can still be overridden without issue:

```py
from typing import Final

class Base:
    FINAL: Final[int] = 1
    NOT_FINAL: int = 2

class Derived(Base):
    FINAL = 3  # error: [override-of-final-variable] "Cannot override final variable `FINAL` from superclass `Base`"
    NOT_FINAL = 4  # No error: not declared as Final
```

### Diamond inheritance

When a `Final` variable is inherited through a diamond, only one error should be reported,
attributing it to the first class in the MRO that defines the `Final` variable:

```py
from typing import Final

class A:
    X: Final[int] = 1

class B(A): ...
class C(A): ...

class D(B, C):
    X = 2  # error: [override-of-final-variable] "Cannot override final variable `X` from superclass `A`"
```

### Multiple `Final` variables from different bases

```py
from typing import Final

class Base1:
    X: Final[int] = 1
    Y: int = 2

class Base2:
    Y: Final[str] = "hello"
    Z: Final[float] = 3.0

class Child(Base1, Base2):
    # error: [override-of-final-variable]
    X = 10
    # error: [override-of-final-variable]
    Y = 20
    # error: [override-of-final-variable]
    Z = 30.0
```

### Override with a method definition

A method definition in a subclass that shadows a `Final` class variable should also be detected:

```py
from typing import Final

class Base:
    X: Final[int] = 1

class Derived(Base):
    # error: [override-of-final-variable]
    def X(self) -> int:
        return 2
```

### `@override` decorator on `Final` variable override

When a subclass uses `@override` on a method that shadows a `Final` variable from a superclass, the
`override-of-final-variable` diagnostic should still be emitted (because the superclass variable is
`Final`), and no `invalid-explicit-override` error should be raised (because the member does exist
in the superclass). Note that `@override` can only be applied to methods, not variable assignments:

```py
from typing import Final
from typing_extensions import override

class Base:
    X: Final[int] = 1
    Y: Final[int] = 2

class Derived(Base):
    @override
    # error: [override-of-final-variable]
    def X(self) -> int:
        return 2
    # error: [override-of-final-variable]
    Y = 3
```

### Chain of overrides

When multiple levels of subclasses override the same `Final` variable, each override should be
reported individually. The error always attributes the violation to the *first* class in the MRO
that declares the variable as `Final`:

```py
from typing import Final

class A:
    X: Final[int] = 1

class B(A):
    X = 2  # error: [override-of-final-variable] "Cannot override final variable `X` from superclass `A`"

class C(B):
    X = 3  # error: [override-of-final-variable] "Cannot override final variable `X` from superclass `A`"
```

### `ClassVar[Final[...]]` and `Annotated[Final[...]]`

`Final` combined with `ClassVar` or `Annotated` should still prevent overrides:

```py
from typing import Final, ClassVar, Annotated

class Base:
    X: ClassVar[Final[int]] = 1
    Y: Annotated[Final[int], "metadata"] = 2

class Derived(Base):
    # error: [override-of-final-variable]
    X = 10
    # error: [override-of-final-variable]
    Y = 20
```

### Cross-module `Final` variable

`base.py`:

```py
from typing import Final

class Base:
    CONST: Final[int] = 42
```

`derived.py`:

```py
from base import Base

class Derived(Base):
    CONST = 100  # error: [override-of-final-variable]
```

### Superclass with same name as subclass

<!-- snapshot-diagnostics -->

`module_a.py`:

```py
from typing import Final

class Foo:
    X: Final[int] = 1
```

`module_b.py`:

```py
from module_a import Foo as BaseFoo

class Foo(BaseFoo):
    X = 2  # error: [override-of-final-variable]
```

### `Final` declaration without a value

A bare `Final` declaration without an assigned value should still prevent overrides:

```py
from typing import Final

class Base:
    X: Final[int]  # error: [final-without-value]

class Derived(Base):
    X = 1  # error: [override-of-final-variable]
```

### Instance `Final` declared in `__init__`

Instance attributes declared as `Final` in `__init__` should also prevent overrides in subclasses:

```py
from typing import Final

class Base:
    def __init__(self):
        self.x: Final[int] = 1

class Derived(Base):
    # TODO: This should be an error, but instance attribute override checking is not yet supported
    def __init__(self):
        self.x = 2
```

### Private (name-mangled) members are not checked

Name-mangled private members use different underlying names per class, so overrides are allowed:

```py
from typing import Final

class Base:
    __X: Final[int] = 1

class Derived(Base):
    __X = 2  # No error: name mangling means these are different attributes
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

NO_ASSIGNMENT_A: Final  # error: [final-without-value] "`Final` symbol `NO_ASSIGNMENT_A` is not assigned a value"
NO_ASSIGNMENT_B: Final[int]  # error: [final-without-value] "`Final` symbol `NO_ASSIGNMENT_B` is not assigned a value"

class C:
    NO_ASSIGNMENT_A: Final  # error: [final-without-value] "`Final` symbol `NO_ASSIGNMENT_A` is not assigned a value"
    NO_ASSIGNMENT_B: Final[int]  # error: [final-without-value] "`Final` symbol `NO_ASSIGNMENT_B` is not assigned a value"

    DEFINED_IN_INIT: Final[int]

    def __init__(self):
        self.DEFINED_IN_INIT = 1
```

### Function-local `Final` without value

```py
from typing import Final

def f():
    x: Final[int]  # error: [final-without-value] "`Final` symbol `x` is not assigned a value"
```

### `typing_extensions.Final` without value

```py
from typing_extensions import Final

TEXF_NO_VALUE: Final[str]  # error: [final-without-value] "`Final` symbol `TEXF_NO_VALUE` is not assigned a value"
```

### `Annotated[Final[...], ...]` without value

```py
from typing import Annotated, Final

ANNOTATED_FINAL: Annotated[  # error: [final-without-value] "`Final` symbol `ANNOTATED_FINAL` is not assigned a value"
    Final[int], "metadata"
]
```

### Imported `Final` symbol

Importing a symbol that is declared `Final` in its source module should not trigger
`final-without-value`, because the import itself provides the binding.

`module.py`:

```py
from typing import Final

MODULE_FINAL: Final[int] = 1
```

`test.py`:

```py
from module import MODULE_FINAL
```

Even if the imported symbol is later deleted (a common pattern to clean up module namespaces), it
should not trigger the diagnostic.

`test_del.py`:

```py
from module import MODULE_FINAL

_ = MODULE_FINAL

del MODULE_FINAL
```

### Stub file `Final` without value

In stub files, `Final` declarations without a value are permitted, at both module and class scope.

`stub.pyi`:

```pyi
from typing import Final

STUB_FINAL: Final[int]

class StubClass:
    STUB_ATTR: Final[str]
```

### Conditional assignment in `__init__`

A `Final` attribute declared in the class body and conditionally assigned in `__init__` should not
trigger `final-without-value`, since at least one path provides a binding.

```py
from typing import Final

class C:
    x: Final[int]

    def __init__(self, flag: bool):
        if flag:
            self.x = 1
        else:
            self.x = 2

class D:
    y: Final[int]

    def __init__(self, flag: bool):
        if flag:
            self.y = 1
        # No else: y may be unbound at runtime, but there is still an assignment path
```

### Assignment in non-`__init__` method

Per the typing spec, a `Final` attribute declared in a class body without a value must be
initialized in `__init__`. Assignment in other methods does not satisfy the requirement.

```py
from typing import Final

class E:
    x: Final[int]  # error: [final-without-value] "`Final` symbol `x` is not assigned a value"

    def setup(self):
        # error: [invalid-assignment] "Cannot assign to final attribute `x`"
        self.x = 1  # Too late: not __init__
```

### Dataclass with `Final` field

Dataclass-like classes do not report `final-without-value` because the `__init__` is synthesized by
the framework.

```py
from dataclasses import dataclass
from typing import Final

@dataclass
class D:
    x: Final[int]  # No error: dataclass generates __init__
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
    attr6: Final[int]  # error: [final-without-value] "`Final` symbol `attr6` is not assigned a value"

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

`Final` declaration without value:

```py
from typing import Final

UNINITIALIZED: Final[int]  # error: [final-without-value]
```

[`typing.final`]: https://docs.python.org/3/library/typing.html#typing.Final
