# Attributes

Tests for attribute access on various kinds of types.

## Class and instance variables

### Pure instance variables

#### Variable only declared/bound in `__init__`

Variables only declared and/or bound in `__init__` are pure instance variables. They cannot be
accessed on the class itself.

```py
class C:
    def __init__(self, param: int | None, flag: bool = False) -> None:
        value = 1 if flag else "a"
        self.inferred_from_value = value
        self.inferred_from_other_attribute = self.inferred_from_value
        self.inferred_from_param = param
        self.declared_only: bytes
        self.declared_and_bound: bool = True
        if flag:
            self.possibly_undeclared_unbound: str = "possibly set in __init__"

c_instance = C(1)

reveal_type(c_instance.inferred_from_value)  # revealed: Unknown | Literal[1, "a"]

# TODO: Same here. This should be `Unknown | Literal[1, "a"]`
reveal_type(c_instance.inferred_from_other_attribute)  # revealed: Unknown

# There is no special handling of attributes that are (directly) assigned to a declared parameter,
# which means we union with `Unknown` here, since the attribute itself is not declared. This is
# something that we might want to change in the future.
#
# See https://github.com/astral-sh/ruff/issues/15960 for a related discussion.
reveal_type(c_instance.inferred_from_param)  # revealed: Unknown | int | None

reveal_type(c_instance.declared_only)  # revealed: bytes

reveal_type(c_instance.declared_and_bound)  # revealed: bool

# We probably don't want to emit a diagnostic for this being possibly undeclared/unbound.
# mypy and pyright do not show an error here.
reveal_type(c_instance.possibly_undeclared_unbound)  # revealed: str

# This assignment is fine, as we infer `Unknown | Literal[1, "a"]` for `inferred_from_value`.
c_instance.inferred_from_value = "value set on instance"

# This assignment is also fine:
c_instance.declared_and_bound = False

# error: [invalid-assignment] "Object of type `Literal["incompatible"]` is not assignable to attribute `declared_and_bound` of type `bool`"
c_instance.declared_and_bound = "incompatible"

# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
# error: [unresolved-attribute] "Attribute `inferred_from_value` can only be accessed on instances, not on the class object `Literal[C]` itself."
reveal_type(C.inferred_from_value)  # revealed: Unknown

# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
# error: [invalid-attribute-access] "Cannot assign to instance attribute `inferred_from_value` from the class object `Literal[C]`"
C.inferred_from_value = "overwritten on class"

# This assignment is fine:
c_instance.declared_and_bound = False

# TODO: After this assignment to the attribute within this scope, we may eventually want to narrow
# the `bool` type (see above) for this instance variable to `Literal[False]` here. This is unsound
# in general (we don't know what else happened to `c_instance` between the assignment and the use
# here), but mypy and pyright support this. In conclusion, this could be `bool` but should probably
# be `Literal[False]`.
reveal_type(c_instance.declared_and_bound)  # revealed: bool
```

#### Variable declared in class body and possibly bound in `__init__`

The same rule applies even if the variable is *declared* (not bound!) in the class body: it is still
a pure instance variable.

```py
class C:
    declared_and_bound: str | None

    def __init__(self) -> None:
        self.declared_and_bound = "value set in __init__"

c_instance = C()

reveal_type(c_instance.declared_and_bound)  # revealed: str | None

# Note that both mypy and pyright show no error in this case! So we may reconsider this in
# the future, if it turns out to produce too many false positives. We currently emit:
# error: [unresolved-attribute] "Attribute `declared_and_bound` can only be accessed on instances, not on the class object `Literal[C]` itself."
reveal_type(C.declared_and_bound)  # revealed: Unknown

# Same as above. Mypy and pyright do not show an error here.
# error: [invalid-attribute-access] "Cannot assign to instance attribute `declared_and_bound` from the class object `Literal[C]`"
C.declared_and_bound = "overwritten on class"

# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `declared_and_bound` of type `str | None`"
c_instance.declared_and_bound = 1
```

#### Variable declared in class body and not bound anywhere

If a variable is declared in the class body but not bound anywhere, we still consider it a pure
instance variable and allow access to it via instances.

```py
class C:
    only_declared: str

c_instance = C()

reveal_type(c_instance.only_declared)  # revealed: str

# Mypy and pyright do not show an error here. We treat this as a pure instance variable.
# error: [unresolved-attribute] "Attribute `only_declared` can only be accessed on instances, not on the class object `Literal[C]` itself."
reveal_type(C.only_declared)  # revealed: Unknown

# error: [invalid-attribute-access] "Cannot assign to instance attribute `only_declared` from the class object `Literal[C]`"
C.only_declared = "overwritten on class"
```

#### Mixed declarations/bindings in class body and `__init__`

```py
class C:
    only_declared_in_body: str | None
    declared_in_body_and_init: str | None

    declared_in_body_defined_in_init: str | None

    bound_in_body_declared_in_init = "a"

    bound_in_body_and_init = None

    def __init__(self, flag) -> None:
        self.only_declared_in_init: str | None
        self.declared_in_body_and_init: str | None = None

        self.declared_in_body_defined_in_init = "a"

        self.bound_in_body_declared_in_init: str | None

        if flag:
            self.bound_in_body_and_init = "a"

c_instance = C(True)

reveal_type(c_instance.only_declared_in_body)  # revealed: str | None
reveal_type(c_instance.only_declared_in_init)  # revealed: str | None
reveal_type(c_instance.declared_in_body_and_init)  # revealed: str | None

reveal_type(c_instance.declared_in_body_defined_in_init)  # revealed: str | None

reveal_type(c_instance.bound_in_body_declared_in_init)  # revealed: str | None

reveal_type(c_instance.bound_in_body_and_init)  # revealed: Unknown | None | Literal["a"]
```

#### Variable defined in non-`__init__` method

We also recognize pure instance variables if they are defined in a method that is not `__init__`.

```py
class C:
    def __init__(self, param: int | None, flag: bool = False) -> None:
        self.initialize(param, flag)

    def initialize(self, param: int | None, flag: bool) -> None:
        value = 1 if flag else "a"
        self.inferred_from_value = value
        self.inferred_from_other_attribute = self.inferred_from_value
        self.inferred_from_param = param
        self.declared_only: bytes
        self.declared_and_bound: bool = True

c_instance = C(1)

reveal_type(c_instance.inferred_from_value)  # revealed: Unknown | Literal[1, "a"]

# TODO: Should be `Unknown | Literal[1, "a"]`
reveal_type(c_instance.inferred_from_other_attribute)  # revealed: Unknown

reveal_type(c_instance.inferred_from_param)  # revealed: Unknown | int | None

reveal_type(c_instance.declared_only)  # revealed: bytes

reveal_type(c_instance.declared_and_bound)  # revealed: bool

# error: [unresolved-attribute] "Attribute `inferred_from_value` can only be accessed on instances, not on the class object `Literal[C]` itself."
reveal_type(C.inferred_from_value)  # revealed: Unknown

# error: [invalid-attribute-access] "Cannot assign to instance attribute `inferred_from_value` from the class object `Literal[C]`"
C.inferred_from_value = "overwritten on class"
```

#### Variable defined in multiple methods

If we see multiple un-annotated assignments to a single attribute (`self.x` below), we build the
union of all inferred types (and `Unknown`). If we see multiple conflicting declarations of the same
attribute, that should be an error.

```py
def get_int() -> int:
    return 0

def get_str() -> str:
    return "a"

class C:
    z: int

    def __init__(self) -> None:
        self.x = get_int()
        self.y: int = 1

    def other_method(self):
        self.x = get_str()

        # TODO: this redeclaration should be an error
        self.y: str = "a"

        # TODO: this redeclaration should be an error
        self.z: str = "a"

c_instance = C()

reveal_type(c_instance.x)  # revealed: Unknown | int | str
reveal_type(c_instance.y)  # revealed: int
reveal_type(c_instance.z)  # revealed: int
```

#### Attributes defined in multi-target assignments

```py
class C:
    def __init__(self) -> None:
        self.a = self.b = 1

c_instance = C()

reveal_type(c_instance.a)  # revealed: Unknown | Literal[1]
reveal_type(c_instance.b)  # revealed: Unknown | Literal[1]
```

#### Augmented assignments

```py
class Weird:
    def __iadd__(self, other: None) -> str:
        return "a"

class C:
    def __init__(self) -> None:
        self.w = Weird()
        self.w += None

# TODO: Mypy and pyright do not support this, but it would be great if we could
# infer `Unknown | str` or at least `Unknown | Weird | str` here.
reveal_type(C().w)  # revealed: Unknown | Weird
```

#### Attributes defined in tuple unpackings

```py
def returns_tuple() -> tuple[int, str]:
    return (1, "a")

class C:
    a1, b1 = (1, "a")
    c1, d1 = returns_tuple()

    def __init__(self) -> None:
        self.a2, self.b2 = (1, "a")
        self.c2, self.d2 = returns_tuple()

c_instance = C()

reveal_type(c_instance.a1)  # revealed: Unknown | Literal[1]
reveal_type(c_instance.b1)  # revealed: Unknown | Literal["a"]
reveal_type(c_instance.c1)  # revealed: Unknown | int
reveal_type(c_instance.d1)  # revealed: Unknown | str

reveal_type(c_instance.a2)  # revealed: Unknown | Literal[1]

reveal_type(c_instance.b2)  # revealed: Unknown | Literal["a"]

reveal_type(c_instance.c2)  # revealed: Unknown | int
reveal_type(c_instance.d2)  # revealed: Unknown | str
```

#### Starred assignments

```py
class C:
    def __init__(self) -> None:
        self.a, *self.b = (1, 2, 3)

c_instance = C()
reveal_type(c_instance.a)  # revealed: Unknown | Literal[1]
reveal_type(c_instance.b)  # revealed: Unknown | @Todo(starred unpacking)
```

#### Attributes defined in for-loop (unpacking)

```py
class IntIterator:
    def __next__(self) -> int:
        return 1

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

class TupleIterator:
    def __next__(self) -> tuple[int, str]:
        return (1, "a")

class TupleIterable:
    def __iter__(self) -> TupleIterator:
        return TupleIterator()

class NonIterable: ...

class C:
    def __init__(self):
        for self.x in IntIterable():
            pass

        for _, self.y in TupleIterable():
            pass

        # TODO: We should emit a diagnostic here
        for self.z in NonIterable():
            pass

reveal_type(C().x)  # revealed: Unknown | int

reveal_type(C().y)  # revealed: Unknown | str
```

#### Attributes defined in `with` statements

```py
class ContextManager:
    def __enter__(self) -> int | None: ...
    def __exit__(self, exc_type, exc_value, traceback) -> None: ...

class C:
    def __init__(self) -> None:
        with ContextManager() as self.x:
            pass

c_instance = C()

# TODO: Should be `Unknown | int | None`
# error: [unresolved-attribute]
reveal_type(c_instance.x)  # revealed: Unknown
```

#### Attributes defined in comprehensions

```py
class IntIterator:
    def __next__(self) -> int:
        return 1

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

class C:
    def __init__(self) -> None:
        [... for self.a in IntIterable()]

c_instance = C()

# TODO: Should be `Unknown | int`
# error: [unresolved-attribute]
reveal_type(c_instance.a)  # revealed: Unknown
```

#### Conditionally declared / bound attributes

We currently do not raise a diagnostic or change behavior if an attribute is only conditionally
defined. This is consistent with what mypy and pyright do.

```py
def flag() -> bool:
    return True

class C:
    def f(self) -> None:
        if flag():
            self.a1: str | None = "a"
            self.b1 = 1
    if flag():
        def f(self) -> None:
            self.a2: str | None = "a"
            self.b2 = 1

c_instance = C()

reveal_type(c_instance.a1)  # revealed: str | None
reveal_type(c_instance.a2)  # revealed: str | None
reveal_type(c_instance.b1)  # revealed: Unknown | Literal[1]
reveal_type(c_instance.b2)  # revealed: Unknown | Literal[1]
```

#### Methods that does not use `self` as a first parameter

```py
class C:
    # This might trigger a stylistic lint like `invalid-first-argument-name-for-method`, but
    # it should be supported in general:
    def __init__(this) -> None:
        this.declared_and_bound: str | None = "a"

reveal_type(C().declared_and_bound)  # revealed: str | None
```

#### Aliased `self` parameter

```py
class C:
    def __init__(self) -> None:
        this = self
        this.declared_and_bound: str | None = "a"

# This would ideally be `str | None`, but mypy/pyright don't support this either,
# so `Unknown` + a diagnostic is also fine.
# error: [unresolved-attribute]
reveal_type(C().declared_and_bound)  # revealed: Unknown
```

#### Static methods do not influence implicitly defined attributes

```py
class Other:
    x: int

class C:
    @staticmethod
    def f(other: Other) -> None:
        other.x = 1

# error: [unresolved-attribute]
reveal_type(C.x)  # revealed: Unknown

# TODO: this should raise `unresolved-attribute` as well, and the type should be `Unknown`
reveal_type(C().x)  # revealed: Unknown | Literal[1]

# This also works if `staticmethod` is aliased:

my_staticmethod = staticmethod

class D:
    @my_staticmethod
    def f(other: Other) -> None:
        other.x = 1

# error: [unresolved-attribute]
reveal_type(D.x)  # revealed: Unknown

# TODO: this should raise `unresolved-attribute` as well, and the type should be `Unknown`
reveal_type(D().x)  # revealed: Unknown | Literal[1]
```

If `staticmethod` is something else, that should not influence the behavior:

```py
def staticmethod(f):
    return f

class C:
    @staticmethod
    def f(self) -> None:
        self.x = 1

reveal_type(C().x)  # revealed: Unknown | Literal[1]
```

And if `staticmethod` is fully qualified, that should also be recognized:

```py
import builtins

class Other:
    x: int

class C:
    @builtins.staticmethod
    def f(other: Other) -> None:
        other.x = 1

# error: [unresolved-attribute]
reveal_type(C.x)  # revealed: Unknown

# TODO: this should raise `unresolved-attribute` as well, and the type should be `Unknown`
reveal_type(C().x)  # revealed: Unknown | Literal[1]
```

#### Attributes defined in statically-known-to-be-false branches

```py
class C:
    def __init__(self) -> None:
        # We use a "significantly complex" condition here (instead of just `False`)
        # for a proper comparison with mypy and pyright, which distinguish between
        # conditions that can be resolved from a simple pattern matching and those
        # that need proper type inference.
        if (2 + 3) < 4:
            self.x: str = "a"

# TODO: Ideally, this would result in a `unresolved-attribute` error. But mypy and pyright
# do not support this either (for conditions that can only be resolved to `False` in type
# inference), so it does not seem to be particularly important.
reveal_type(C().x)  # revealed: str
```

#### Diagnostics are reported for the right-hand side of attribute assignments

```py
class C:
    def __init__(self) -> None:
        # error: [too-many-positional-arguments]
        self.x: int = len(1, 2, 3)
```

### Pure class variables (`ClassVar`)

#### Annotated with `ClassVar` type qualifier

Class variables annotated with the [`typing.ClassVar`] type qualifier are pure class variables. They
cannot be overwritten on instances, but they can be accessed on instances.

For more details, see the [typing spec on `ClassVar`].

```py
from typing import ClassVar

class C:
    pure_class_variable1: ClassVar[str] = "value in class body"
    pure_class_variable2: ClassVar = 1

    def method(self):
        # TODO: this should be an error
        self.pure_class_variable1 = "value set through instance"

reveal_type(C.pure_class_variable1)  # revealed: str

# TODO: Should be `Unknown | Literal[1]`.
reveal_type(C.pure_class_variable2)  # revealed: Unknown

c_instance = C()

# It is okay to access a pure class variable on an instance.
reveal_type(c_instance.pure_class_variable1)  # revealed: str

# TODO: Should be `Unknown | Literal[1]`.
reveal_type(c_instance.pure_class_variable2)  # revealed: Unknown

# error: [invalid-attribute-access] "Cannot assign to ClassVar `pure_class_variable1` from an instance of type `C`"
c_instance.pure_class_variable1 = "value set on instance"

C.pure_class_variable1 = "overwritten on class"

# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `pure_class_variable1` of type `str`"
C.pure_class_variable1 = 1

class Subclass(C):
    pure_class_variable1: ClassVar[str] = "overwritten on subclass"

reveal_type(Subclass.pure_class_variable1)  # revealed: str
```

#### Variable only mentioned in a class method

We also consider a class variable to be a pure class variable if it is only mentioned in a class
method.

```py
class C:
    @classmethod
    def class_method(cls):
        cls.pure_class_variable = "value set in class method"

# for a more realistic example, let's actually call the method
C.class_method()

# TODO: We currently plan to support this and show no error here.
# mypy shows an error here, pyright does not.
# error: [unresolved-attribute]
reveal_type(C.pure_class_variable)  # revealed: Unknown

# TODO: should be no error when descriptor protocol is supported
# and the assignment is properly attributed to the class method.
# error: [invalid-attribute-access] "Cannot assign to instance attribute `pure_class_variable` from the class object `Literal[C]`"
C.pure_class_variable = "overwritten on class"

# TODO: should be  `Unknown | Literal["value set in class method"]` or
# Literal["overwritten on class"]`, once/if we support local narrowing.
# error: [unresolved-attribute]
reveal_type(C.pure_class_variable)  # revealed: Unknown

c_instance = C()
reveal_type(c_instance.pure_class_variable)  # revealed: Unknown | Literal["value set in class method"]

# TODO: should raise an error.
c_instance.pure_class_variable = "value set on instance"
```

### Instance variables with class-level default values

These are instance attributes, but the fact that we can see that they have a binding (not a
declaration) in the class body means that reading the value from the class directly is also
permitted. This is the only difference for these attributes as opposed to "pure" instance
attributes.

#### Basic

```py
class C:
    variable_with_class_default1: str = "value in class body"
    variable_with_class_default2 = 1

    def instance_method(self):
        self.variable_with_class_default1 = "value set in instance method"

reveal_type(C.variable_with_class_default1)  # revealed: str

reveal_type(C.variable_with_class_default2)  # revealed: Unknown | Literal[1]

c_instance = C()

reveal_type(c_instance.variable_with_class_default1)  # revealed: str
reveal_type(c_instance.variable_with_class_default2)  # revealed: Unknown | Literal[1]

c_instance.variable_with_class_default1 = "value set on instance"

reveal_type(C.variable_with_class_default1)  # revealed: str

# TODO: Could be Literal["value set on instance"], or still `str` if we choose not to
# narrow the type.
reveal_type(c_instance.variable_with_class_default1)  # revealed: str

C.variable_with_class_default1 = "overwritten on class"

# TODO: Could be `Literal["overwritten on class"]`, or still `str` if we choose not to
# narrow the type.
reveal_type(C.variable_with_class_default1)  # revealed: str

# TODO: should still be `Literal["value set on instance"]`, or `str`.
reveal_type(c_instance.variable_with_class_default1)  # revealed: str
```

### Inheritance of class/instance attributes

#### Instance variable defined in a base class

```py
class Base:
    declared_in_body: int | None = 1

    base_class_attribute_1: str | None
    base_class_attribute_2: str | None
    base_class_attribute_3: str | None

    def __init__(self) -> None:
        self.defined_in_init: str | None = "value in base"

class Intermediate(Base):
    # Re-declaring base class attributes with the *same *type is fine:
    base_class_attribute_1: str | None = None

    # Re-declaring them with a *narrower type* is unsound, because modifications
    # through a `Base` reference could violate that constraint.
    #
    # Mypy does not report an error here, but pyright does: "… overrides symbol
    # of same name in class "Base". Variable is mutable so its type is invariant"
    #
    # We should introduce a diagnostic for this. Whether or not that should be
    # enabled by default can still be discussed.
    #
    # TODO: This should be an error
    base_class_attribute_2: str

    # Re-declaring attributes with a *wider type* directly violates LSP.
    #
    # In this case, both mypy and pyright report an error.
    #
    # TODO: This should be an error
    base_class_attribute_3: str | int | None

class Derived(Intermediate): ...

reveal_type(Derived.declared_in_body)  # revealed: int | None

reveal_type(Derived().declared_in_body)  # revealed: int | None

reveal_type(Derived().defined_in_init)  # revealed: str | None
```

## Union of attributes

```py
def _(flag: bool):
    if flag:
        class C1:
            x = 1

    else:
        class C1:
            x = 2

    class C2:
        if flag:
            x = 3
        else:
            x = 4

    reveal_type(C1.x)  # revealed: Unknown | Literal[1, 2]
    reveal_type(C2.x)  # revealed: Unknown | Literal[3, 4]
```

## Inherited class attributes

### Basic

```py
class A:
    X = "foo"

class B(A): ...
class C(B): ...

reveal_type(C.X)  # revealed: Unknown | Literal["foo"]
```

### Multiple inheritance

```py
class O: ...

class F(O):
    X = 56

class E(O):
    X = 42

class D(O): ...
class C(D, F): ...
class B(E, D): ...
class A(B, C): ...

# revealed: tuple[Literal[A], Literal[B], Literal[E], Literal[C], Literal[D], Literal[F], Literal[O], Literal[object]]
reveal_type(A.__mro__)

# `E` is earlier in the MRO than `F`, so we should use the type of `E.X`
reveal_type(A.X)  # revealed: Unknown | Literal[42]
```

## Unions with possibly unbound paths

### Definite boundness within a class

In this example, the `x` attribute is not defined in the `C2` element of the union:

```py
def _(flag1: bool, flag2: bool):
    class C1:
        x = 1

    class C2: ...

    class C3:
        x = 3

    C = C1 if flag1 else C2 if flag2 else C3

    # error: [possibly-unbound-attribute] "Attribute `x` on type `Literal[C1, C2, C3]` is possibly unbound"
    reveal_type(C.x)  # revealed: Unknown | Literal[1, 3]
```

### Possibly-unbound within a class

We raise the same diagnostic if the attribute is possibly-unbound in at least one element of the
union:

```py
def _(flag: bool, flag1: bool, flag2: bool):
    class C1:
        x = 1

    class C2:
        if flag:
            x = 2

    class C3:
        x = 3

    C = C1 if flag1 else C2 if flag2 else C3

    # error: [possibly-unbound-attribute] "Attribute `x` on type `Literal[C1, C2, C3]` is possibly unbound"
    reveal_type(C.x)  # revealed: Unknown | Literal[1, 2, 3]
```

### Attribute possibly unbound on a subclass but not on a superclass

```py
def _(flag: bool):
    class Foo:
        x = 1

    class Bar(Foo):
        if flag:
            x = 2

    reveal_type(Bar.x)  # revealed: Unknown | Literal[2, 1]
```

### Attribute possibly unbound on a subclass and on a superclass

```py
def _(flag: bool):
    class Foo:
        if flag:
            x = 1

    class Bar(Foo):
        if flag:
            x = 2

    # error: [possibly-unbound-attribute]
    reveal_type(Bar.x)  # revealed: Unknown | Literal[2, 1]
```

### Attribute access on `Any`

The union of the set of types that `Any` could materialise to is equivalent to `object`. It follows
from this that attribute access on `Any` resolves to `Any` if the attribute does not exist on
`object` -- but if the attribute *does* exist on `object`, the type of the attribute is
`<type as it exists on object> & Any`.

```py
from typing import Any

class Foo(Any): ...

reveal_type(Foo.bar)  # revealed: Any
reveal_type(Foo.__repr__)  # revealed: Literal[__repr__] & Any
```

Similar principles apply if `Any` appears in the middle of an inheritance hierarchy:

```py
from typing import ClassVar, Literal

class A:
    x: ClassVar[Literal[1]] = 1

class B(Any): ...
class C(B, A): ...

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[B], Any, Literal[A], Literal[object]]
reveal_type(C.x)  # revealed: Literal[1] & Any
```

### Unions with all paths unbound

If the symbol is unbound in all elements of the union, we detect that:

```py
def _(flag: bool):
    class C1: ...
    class C2: ...
    C = C1 if flag else C2

    # error: [unresolved-attribute] "Type `Literal[C1, C2]` has no attribute `x`"
    reveal_type(C.x)  # revealed: Unknown
```

## Objects of all types have a `__class__` method

The type of `x.__class__` is the same as `x`'s meta-type. `x.__class__` is always the same value as
`type(x)`.

```py
import typing_extensions

reveal_type(typing_extensions.__class__)  # revealed: Literal[ModuleType]
reveal_type(type(typing_extensions))  # revealed: Literal[ModuleType]

a = 42
reveal_type(a.__class__)  # revealed: Literal[int]
reveal_type(type(a))  # revealed: Literal[int]

b = "42"
reveal_type(b.__class__)  # revealed: Literal[str]

c = b"42"
reveal_type(c.__class__)  # revealed: Literal[bytes]

d = True
reveal_type(d.__class__)  # revealed: Literal[bool]

e = (42, 42)
reveal_type(e.__class__)  # revealed: Literal[tuple]

def f(a: int, b: typing_extensions.LiteralString, c: int | str, d: type[str]):
    reveal_type(a.__class__)  # revealed: type[int]
    reveal_type(type(a))  # revealed: type[int]

    reveal_type(b.__class__)  # revealed: Literal[str]
    reveal_type(type(b))  # revealed: Literal[str]

    reveal_type(c.__class__)  # revealed: type[int] | type[str]
    reveal_type(type(c))  # revealed: type[int] | type[str]

    # `type[type]`, a.k.a., either the class `type` or some subclass of `type`.
    # It would be incorrect to infer `Literal[type]` here,
    # as `c` could be some subclass of `str` with a custom metaclass.
    # All we know is that the metaclass must be a (non-strict) subclass of `type`.
    reveal_type(d.__class__)  # revealed: type[type]

reveal_type(f.__class__)  # revealed: Literal[FunctionType]

class Foo: ...

reveal_type(Foo.__class__)  # revealed: Literal[type]
```

## Module attributes

`mod.py`:

```py
global_symbol: str = "a"
```

```py
import mod

reveal_type(mod.global_symbol)  # revealed: str
mod.global_symbol = "b"

# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `global_symbol` of type `str`"
mod.global_symbol = 1

# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `global_symbol` of type `str`"
(_, mod.global_symbol) = (..., 1)

# TODO: this should be an error, but we do not understand list unpackings yet.
[_, mod.global_symbol] = [1, 2]

class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

# error: [invalid-assignment] "Object of type `int` is not assignable to attribute `global_symbol` of type `str`"
for mod.global_symbol in IntIterable():
    pass
```

## Nested attributes

`outer/__init__.py`:

```py
```

`outer/nested/__init__.py`:

```py
```

`outer/nested/inner.py`:

```py
class Outer:
    class Nested:
        class Inner:
            attr: int = 1
```

```py
import outer.nested.inner

reveal_type(outer.nested.inner.Outer.Nested.Inner.attr)  # revealed: int

# error: [invalid-assignment]
outer.nested.inner.Outer.Nested.Inner.attr = "a"
```

## Literal types

### Function-literal attributes

Most attribute accesses on function-literal types are delegated to `types.FunctionType`, since all
functions are instances of that class:

```py
def f(): ...

reveal_type(f.__defaults__)  # revealed: @Todo(full tuple[...] support) | None
reveal_type(f.__kwdefaults__)  # revealed: @Todo(generics) | None
```

Some attributes are special-cased, however:

```py
reveal_type(f.__get__)  # revealed: <method-wrapper `__get__` of `f`>
reveal_type(f.__call__)  # revealed: <bound method `__call__` of `Literal[f]`>
```

### Int-literal attributes

Most attribute accesses on int-literal types are delegated to `builtins.int`, since all literal
integers are instances of that class:

```py
reveal_type((2).bit_length)  # revealed: <bound method `bit_length` of `Literal[2]`>
reveal_type((2).denominator)  # revealed: @Todo(@property)
```

Some attributes are special-cased, however:

```py
reveal_type((2).numerator)  # revealed: Literal[2]
reveal_type((2).real)  # revealed: Literal[2]
```

### Bool-literal attributes

Most attribute accesses on bool-literal types are delegated to `builtins.bool`, since all literal
bools are instances of that class:

```py
reveal_type(True.__and__)  # revealed: @Todo(overloaded method)
reveal_type(False.__or__)  # revealed: @Todo(overloaded method)
```

Some attributes are special-cased, however:

```py
reveal_type(True.numerator)  # revealed: Literal[1]
reveal_type(False.real)  # revealed: Literal[0]
```

### Bytes-literal attributes

All attribute access on literal `bytes` types is currently delegated to `builtins.bytes`:

```py
reveal_type(b"foo".join)  # revealed: <bound method `join` of `Literal[b"foo"]`>
reveal_type(b"foo".endswith)  # revealed: <bound method `endswith` of `Literal[b"foo"]`>
```

## Instance attribute edge cases

### Assignment to attribute that does not correspond to the instance

```py
class Other:
    x: int = 1

class C:
    def __init__(self, other: Other) -> None:
        other.x = 1

def f(c: C):
    # error: [unresolved-attribute]
    reveal_type(c.x)  # revealed: Unknown
```

### Nested classes

```py
class Outer:
    def __init__(self):
        self.x: int = 1

    class Middle:
        # has no 'x' attribute

        class Inner:
            def __init__(self):
                self.x: str = "a"

reveal_type(Outer().x)  # revealed: int

# error: [unresolved-attribute]
Outer.Middle().x

reveal_type(Outer.Middle.Inner().x)  # revealed: str
```

### Shadowing of `self`

```py
class Other:
    x: int = 1

class C:
    def __init__(self) -> None:
        # Redeclaration of self. `self` does not refer to the instance anymore.
        self: Other = Other()
        self.x: int = 1

# TODO: this should be an error
C().x
```

### Assignment to `self` after nested function

```py
class Other:
    x: str = "a"

class C:
    def __init__(self) -> None:
        def nested_function(self: Other):
            self.x = "b"
        self.x: int = 1

reveal_type(C().x)  # revealed: int
```

### Assignment to `self` from nested function

```py
class C:
    def __init__(self) -> None:
        def set_attribute(value: str):
            self.x: str = value
        set_attribute("a")

# TODO: ideally, this would be `str`. Mypy supports this, pyright does not.
# error: [unresolved-attribute]
reveal_type(C().x)  # revealed: Unknown
```

### Builtin types attributes

This test can probably be removed eventually, but we currently include it because we do not yet
understand generic bases and protocols, and we want to make sure that we can still use builtin types
in our tests in the meantime. See the corresponding TODO in `Type::static_member` for more
information.

```py
class C:
    a_int: int = 1
    a_str: str = "a"
    a_bytes: bytes = b"a"
    a_bool: bool = True
    a_float: float = 1.0
    a_complex: complex = 1 + 1j
    a_tuple: tuple[int] = (1,)
    a_range: range = range(1)
    a_slice: slice = slice(1)
    a_type: type = int
    a_none: None = None

reveal_type(C.a_int)  # revealed: int
reveal_type(C.a_str)  # revealed: str
reveal_type(C.a_bytes)  # revealed: bytes
reveal_type(C.a_bool)  # revealed: bool
reveal_type(C.a_float)  # revealed: int | float
reveal_type(C.a_complex)  # revealed: int | float | complex
reveal_type(C.a_tuple)  # revealed: tuple[int]
reveal_type(C.a_range)  # revealed: range
reveal_type(C.a_slice)  # revealed: slice
reveal_type(C.a_type)  # revealed: type
reveal_type(C.a_none)  # revealed: None
```

## References

Some of the tests in the *Class and instance variables* section draw inspiration from
[pyright's documentation] on this topic.

[pyright's documentation]: https://microsoft.github.io/pyright/#/type-concepts-advanced?id=class-and-instance-variables
[typing spec on `classvar`]: https://typing.readthedocs.io/en/latest/spec/class-compat.html#classvar
[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
