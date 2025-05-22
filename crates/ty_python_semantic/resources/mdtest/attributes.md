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

# error: [possibly-unbound-attribute]
reveal_type(c_instance.possibly_undeclared_unbound)  # revealed: str

# This assignment is fine, as we infer `Unknown | Literal[1, "a"]` for `inferred_from_value`.
c_instance.inferred_from_value = "value set on instance"

# This assignment is also fine:
c_instance.declared_and_bound = False

# error: [invalid-assignment] "Object of type `Literal["incompatible"]` is not assignable to attribute `declared_and_bound` of type `bool`"
c_instance.declared_and_bound = "incompatible"

# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
# error: [unresolved-attribute] "Attribute `inferred_from_value` can only be accessed on instances, not on the class object `<class 'C'>` itself."
reveal_type(C.inferred_from_value)  # revealed: Unknown

# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
# error: [invalid-attribute-access] "Cannot assign to instance attribute `inferred_from_value` from the class object `<class 'C'>`"
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
# error: [unresolved-attribute] "Attribute `declared_and_bound` can only be accessed on instances, not on the class object `<class 'C'>` itself."
reveal_type(C.declared_and_bound)  # revealed: Unknown

# Same as above. Mypy and pyright do not show an error here.
# error: [invalid-attribute-access] "Cannot assign to instance attribute `declared_and_bound` from the class object `<class 'C'>`"
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
# error: [unresolved-attribute] "Attribute `only_declared` can only be accessed on instances, not on the class object `<class 'C'>` itself."
reveal_type(C.only_declared)  # revealed: Unknown

# error: [invalid-attribute-access] "Cannot assign to instance attribute `only_declared` from the class object `<class 'C'>`"
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

# TODO: This should be `str | None`. Fixing this requires an overhaul of the `Symbol` API,
# which is planned in https://github.com/astral-sh/ruff/issues/14297
reveal_type(c_instance.bound_in_body_declared_in_init)  # revealed: Unknown | str | None

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

# error: [unresolved-attribute] "Attribute `inferred_from_value` can only be accessed on instances, not on the class object `<class 'C'>` itself."
reveal_type(C.inferred_from_value)  # revealed: Unknown

# error: [invalid-attribute-access] "Cannot assign to instance attribute `inferred_from_value` from the class object `<class 'C'>`"
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
reveal_type(c_instance.b)  # revealed: Unknown
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

# Iterable might be empty
# error: [possibly-unbound-attribute]
reveal_type(C().x)  # revealed: Unknown | int
# error: [possibly-unbound-attribute]
reveal_type(C().y)  # revealed: Unknown | str
```

#### Attributes defined in `with` statements

```py
class ContextManager:
    def __enter__(self) -> int | None:
        return 1

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        pass

class C:
    def __init__(self) -> None:
        with ContextManager() as self.x:
            pass

c_instance = C()

reveal_type(c_instance.x)  # revealed: Unknown | int | None
```

#### Attributes defined in `with` statements, but with unpacking

```py
class ContextManager:
    def __enter__(self) -> tuple[int | None, int]:
        return 1, 2

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        pass

class C:
    def __init__(self) -> None:
        with ContextManager() as (self.x, self.y):
            pass

c_instance = C()

reveal_type(c_instance.x)  # revealed: Unknown | int | None
reveal_type(c_instance.y)  # revealed: Unknown | int
```

#### Attributes defined in comprehensions

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

class C:
    def __init__(self) -> None:
        [... for self.a in IntIterable()]
        [... for (self.b, self.c) in TupleIterable()]
        [... for self.d in IntIterable() for self.e in IntIterable()]

c_instance = C()

reveal_type(c_instance.a)  # revealed: Unknown | int
reveal_type(c_instance.b)  # revealed: Unknown | int
reveal_type(c_instance.c)  # revealed: Unknown | str
reveal_type(c_instance.d)  # revealed: Unknown | int
reveal_type(c_instance.e)  # revealed: Unknown | int
```

#### Conditionally declared / bound attributes

Attributes are possibly unbound if they, or the method to which they are added are conditionally
declared / bound.

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

# error: [possibly-unbound-attribute]
reveal_type(c_instance.a1)  # revealed: str | None
# error: [possibly-unbound-attribute]
reveal_type(c_instance.a2)  # revealed: str | None
# error: [possibly-unbound-attribute]
reveal_type(c_instance.b1)  # revealed: Unknown | Literal[1]
# error: [possibly-unbound-attribute]
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

# error: [unresolved-attribute]
reveal_type(C().x)  # revealed: Unknown
```

```py
class C:
    def __init__(self, cond: bool) -> None:
        if True:
            self.a = 1
        else:
            self.a = "a"

        if False:
            self.b = 2

        if cond:
            return

        self.c = 3

        self.d = 4
        self.d = 5

    def set_c(self, c: str) -> None:
        self.c = c
    if False:
        def set_e(self, e: str) -> None:
            self.e = e

reveal_type(C(True).a)  # revealed: Unknown | Literal[1]
# error: [unresolved-attribute]
reveal_type(C(True).b)  # revealed: Unknown
reveal_type(C(True).c)  # revealed: Unknown | Literal[3] | str
# TODO: this attribute is possibly unbound
reveal_type(C(True).d)  # revealed: Unknown | Literal[5]
# error: [unresolved-attribute]
reveal_type(C(True).e)  # revealed: Unknown
```

#### Attributes considered always bound

```py
class C:
    def __init__(self, cond: bool):
        self.x = 1
        if cond:
            raise ValueError("Something went wrong")

        # We consider this attribute is always bound.
        # This is because, it is not possible to access a partially-initialized object by normal means.
        self.y = 2

reveal_type(C(False).x)  # revealed: Unknown | Literal[1]
reveal_type(C(False).y)  # revealed: Unknown | Literal[2]

class C:
    def __init__(self, b: bytes) -> None:
        self.b = b

        try:
            s = b.decode()
        except UnicodeDecodeError:
            raise ValueError("Invalid UTF-8 sequence")

        self.s = s

reveal_type(C(b"abc").b)  # revealed: Unknown | bytes
reveal_type(C(b"abc").s)  # revealed: Unknown | str

class C:
    def __init__(self, iter) -> None:
        self.x = 1

        for _ in iter:
            pass

        # The for-loop may not stop,
        # but we consider the subsequent attributes to be definitely-bound.
        self.y = 2

reveal_type(C([]).x)  # revealed: Unknown | Literal[1]
reveal_type(C([]).y)  # revealed: Unknown | Literal[2]
```

#### Diagnostics are reported for the right-hand side of attribute assignments

```py
class C:
    def __init__(self) -> None:
        # error: [too-many-positional-arguments]
        # error: [invalid-argument-type]
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
# error: [invalid-attribute-access] "Cannot assign to instance attribute `pure_class_variable` from the class object `<class 'C'>`"
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

#### Descriptor attributes as class variables

Whether they are explicitly qualified as `ClassVar`, or just have a class level default, we treat
descriptor attributes as class variables. This test mainly makes sure that we do *not* treat them as
instance variables. This would lead to a different outcome, since the `__get__` method would not be
called (the descriptor protocol is not invoked for instance variables).

```py
from typing import ClassVar

class Descriptor:
    def __get__(self, instance, owner) -> int:
        return 42

class C:
    a: ClassVar[Descriptor]
    b: Descriptor = Descriptor()
    c: ClassVar[Descriptor] = Descriptor()

reveal_type(C().a)  # revealed: int
reveal_type(C().b)  # revealed: int
reveal_type(C().c)  # revealed: int
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
    # Redeclaring base class attributes with the *same *type is fine:
    base_class_attribute_1: str | None = None

    # Redeclaring them with a *narrower type* is unsound, because modifications
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

    # Redeclaring attributes with a *wider type* directly violates LSP.
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

## Accessing attributes on class objects

When accessing attributes on class objects, they are always looked up on the type of the class
object first, i.e. on the metaclass:

```py
from typing import Literal

class Meta1:
    attr: Literal["metaclass value"] = "metaclass value"

class C1(metaclass=Meta1): ...

reveal_type(C1.attr)  # revealed: Literal["metaclass value"]
```

However, the metaclass attribute only takes precedence over a class-level attribute if it is a data
descriptor. If it is a non-data descriptor or a normal attribute, the class-level attribute is used
instead (see the [descriptor protocol tests] for data/non-data descriptor attributes):

```py
class Meta2:
    attr: str = "metaclass value"

class C2(metaclass=Meta2):
    attr: Literal["class value"] = "class value"

reveal_type(C2.attr)  # revealed: Literal["class value"]
```

If the class-level attribute is only partially defined, we union the metaclass attribute with the
class-level attribute:

```py
def _(flag: bool):
    class Meta3:
        attr1 = "metaclass value"
        attr2: Literal["metaclass value"] = "metaclass value"

    class C3(metaclass=Meta3):
        if flag:
            attr1 = "class value"
            # TODO: Neither mypy nor pyright show an error here, but we could consider emitting a conflicting-declaration diagnostic here.
            attr2: Literal["class value"] = "class value"

    reveal_type(C3.attr1)  # revealed: Unknown | Literal["metaclass value", "class value"]
    reveal_type(C3.attr2)  # revealed: Literal["metaclass value", "class value"]
```

If the *metaclass* attribute is only partially defined, we emit a `possibly-unbound-attribute`
diagnostic:

```py
def _(flag: bool):
    class Meta4:
        if flag:
            attr1: str = "metaclass value"

    class C4(metaclass=Meta4): ...
    # error: [possibly-unbound-attribute]
    reveal_type(C4.attr1)  # revealed: str
```

Finally, if both the metaclass attribute and the class-level attribute are only partially defined,
we union them and emit a `possibly-unbound-attribute` diagnostic:

```py
def _(flag1: bool, flag2: bool):
    class Meta5:
        if flag1:
            attr1 = "metaclass value"

    class C5(metaclass=Meta5):
        if flag2:
            attr1 = "class value"

    # error: [possibly-unbound-attribute]
    reveal_type(C5.attr1)  # revealed: Unknown | Literal["metaclass value", "class value"]
```

## Unions of attributes

If the (meta)class is a union type or if the attribute on the (meta) class has a union type, we
infer those union types accordingly:

```py
def _(flag: bool):
    if flag:
        class C1:
            x = 1
            y: int = 1

    else:
        class C1:
            x = 2
            y: int | str = "b"

    reveal_type(C1.x)  # revealed: Unknown | Literal[1, 2]
    reveal_type(C1.y)  # revealed: int | str

    C1.y = 100
    # error: [invalid-assignment] "Object of type `Literal["problematic"]` is not assignable to attribute `y` on type `<class 'C1'> | <class 'C1'>`"
    C1.y = "problematic"

    class C2:
        if flag:
            x = 3
            y: int = 3
        else:
            x = 4
            y: int | str = "d"

    reveal_type(C2.x)  # revealed: Unknown | Literal[3, 4]
    reveal_type(C2.y)  # revealed: int | str

    C2.y = 100
    # error: [invalid-assignment] "Object of type `None` is not assignable to attribute `y` of type `int | str`"
    C2.y = None
    # TODO: should be an error, needs more sophisticated union handling in `validate_attribute_assignment`
    C2.y = "problematic"

    if flag:
        class Meta3(type):
            x = 5
            y: int = 5

    else:
        class Meta3(type):
            x = 6
            y: int | str = "f"

    class C3(metaclass=Meta3): ...
    reveal_type(C3.x)  # revealed: Unknown | Literal[5, 6]
    reveal_type(C3.y)  # revealed: int | str

    C3.y = 100
    # error: [invalid-assignment] "Object of type `None` is not assignable to attribute `y` of type `int | str`"
    C3.y = None
    # TODO: should be an error, needs more sophisticated union handling in `validate_attribute_assignment`
    C3.y = "problematic"

    class Meta4(type):
        if flag:
            x = 7
            y: int = 7
        else:
            x = 8
            y: int | str = "h"

    class C4(metaclass=Meta4): ...
    reveal_type(C4.x)  # revealed: Unknown | Literal[7, 8]
    reveal_type(C4.y)  # revealed: int | str

    C4.y = 100
    # error: [invalid-assignment] "Object of type `None` is not assignable to attribute `y` of type `int | str`"
    C4.y = None
    # TODO: should be an error, needs more sophisticated union handling in `validate_attribute_assignment`
    C4.y = "problematic"
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

    # error: [possibly-unbound-attribute] "Attribute `x` on type `<class 'C1'> | <class 'C2'> | <class 'C3'>` is possibly unbound"
    reveal_type(C.x)  # revealed: Unknown | Literal[1, 3]

    # error: [invalid-assignment] "Object of type `Literal[100]` is not assignable to attribute `x` on type `<class 'C1'> | <class 'C2'> | <class 'C3'>`"
    C.x = 100

    # error: [possibly-unbound-attribute] "Attribute `x` on type `C1 | C2 | C3` is possibly unbound"
    reveal_type(C().x)  # revealed: Unknown | Literal[1, 3]

    # error: [invalid-assignment] "Object of type `Literal[100]` is not assignable to attribute `x` on type `C1 | C2 | C3`"
    C().x = 100
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

    # error: [possibly-unbound-attribute] "Attribute `x` on type `<class 'C1'> | <class 'C2'> | <class 'C3'>` is possibly unbound"
    reveal_type(C.x)  # revealed: Unknown | Literal[1, 2, 3]

    # error: [possibly-unbound-attribute]
    C.x = 100

    # Note: we might want to consider ignoring possibly-unbound diagnostics for instance attributes eventually,
    # see the "Possibly unbound/undeclared instance attribute" section below.
    # error: [possibly-unbound-attribute] "Attribute `x` on type `C1 | C2 | C3` is possibly unbound"
    reveal_type(C().x)  # revealed: Unknown | Literal[1, 2, 3]

    # error: [possibly-unbound-attribute]
    C().x = 100
```

### Possibly-unbound within gradual types

```py
from typing import Any

def _(flag: bool):
    class Base:
        x: Any

    class Derived(Base):
        if flag:
            # Redeclaring `x` with a more static type is okay in terms of LSP.
            x: int

    reveal_type(Derived().x)  # revealed: int | Any

    Derived().x = 1
    Derived().x = "a"
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
    Bar.x = 3

    reveal_type(Bar().x)  # revealed: Unknown | Literal[2, 1]
    Bar().x = 3
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

    # error: [possibly-unbound-attribute]
    Bar.x = 3

    # error: [possibly-unbound-attribute]
    reveal_type(Bar().x)  # revealed: Unknown | Literal[2, 1]

    # error: [possibly-unbound-attribute]
    Bar().x = 3
```

### Possibly unbound/undeclared instance attribute

#### Possibly unbound and undeclared

```py
def _(flag: bool):
    class Foo:
        if flag:
            x: int

        def __init(self):
            if flag:
                self.x = 1

    # error: [possibly-unbound-attribute]
    reveal_type(Foo().x)  # revealed: int | Unknown

    # error: [possibly-unbound-attribute]
    Foo().x = 1
```

#### Possibly unbound

```py
def _(flag: bool):
    class Foo:
        def __init(self):
            if flag:
                self.x = 1
                self.y = "a"
            else:
                self.y = "b"

    # error: [possibly-unbound-attribute]
    reveal_type(Foo().x)  # revealed: Unknown | Literal[1]

    # error: [possibly-unbound-attribute]
    Foo().x = 2

    reveal_type(Foo().y)  # revealed: Unknown | Literal["a", "b"]
    Foo().y = "c"
```

### Unions with all paths unbound

If the symbol is unbound in all elements of the union, we detect that:

```py
def _(flag: bool):
    class C1: ...
    class C2: ...
    C = C1 if flag else C2

    # error: [unresolved-attribute] "Type `<class 'C1'> | <class 'C2'>` has no attribute `x`"
    reveal_type(C.x)  # revealed: Unknown

    # TODO: This should ideally be a `unresolved-attribute` error. We need better union
    # handling in `validate_attribute_assignment` for this.
    # error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `x` on type `<class 'C1'> | <class 'C2'>`"
    C.x = 1
```

## Inherited class attributes

### Basic

```py
class A:
    X = "foo"

class B(A): ...
class C(B): ...

reveal_type(C.X)  # revealed: Unknown | Literal["foo"]

C.X = "bar"
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

# revealed: tuple[<class 'A'>, <class 'B'>, <class 'E'>, <class 'C'>, <class 'D'>, <class 'F'>, <class 'O'>, <class 'object'>]
reveal_type(A.__mro__)

# `E` is earlier in the MRO than `F`, so we should use the type of `E.X`
reveal_type(A.X)  # revealed: Unknown | Literal[42]

A.X = 100
```

## Intersections of attributes

### Attribute only available on one element

```py
from ty_extensions import Intersection

class A:
    x: int = 1

class B: ...

def _(a_and_b: Intersection[A, B]):
    reveal_type(a_and_b.x)  # revealed: int

    a_and_b.x = 2

# Same for class objects
def _(a_and_b: Intersection[type[A], type[B]]):
    reveal_type(a_and_b.x)  # revealed: int

    a_and_b.x = 2
```

### Attribute available on both elements

```py
from ty_extensions import Intersection

class P: ...
class Q: ...
class R(P, Q): ...

class A:
    x: P = P()

class B:
    x: Q = Q()

def _(a_and_b: Intersection[A, B]):
    reveal_type(a_and_b.x)  # revealed: P & Q
    a_and_b.x = R()

# Same for class objects
def _(a_and_b: Intersection[type[A], type[B]]):
    reveal_type(a_and_b.x)  # revealed: P & Q
    a_and_b.x = R()
```

### Possible unboundness

```py
from ty_extensions import Intersection

class P: ...
class Q: ...
class R(P, Q): ...

def _(flag: bool):
    class A1:
        if flag:
            x: P = P()

    class B1: ...

    def inner1(a_and_b: Intersection[A1, B1]):
        # error: [possibly-unbound-attribute]
        reveal_type(a_and_b.x)  # revealed: P

        # error: [possibly-unbound-attribute]
        a_and_b.x = R()
    # Same for class objects
    def inner1_class(a_and_b: Intersection[type[A1], type[B1]]):
        # error: [possibly-unbound-attribute]
        reveal_type(a_and_b.x)  # revealed: P

        # error: [possibly-unbound-attribute]
        a_and_b.x = R()

    class A2:
        if flag:
            x: P = P()

    class B1:
        x: Q = Q()

    def inner2(a_and_b: Intersection[A2, B1]):
        reveal_type(a_and_b.x)  # revealed: P & Q

        # TODO: this should not be an error, we need better intersection
        # handling in `validate_attribute_assignment` for this
        # error: [possibly-unbound-attribute]
        a_and_b.x = R()
    # Same for class objects
    def inner2_class(a_and_b: Intersection[type[A2], type[B1]]):
        reveal_type(a_and_b.x)  # revealed: P & Q

    class A3:
        if flag:
            x: P = P()

    class B3:
        if flag:
            x: Q = Q()

    def inner3(a_and_b: Intersection[A3, B3]):
        # error: [possibly-unbound-attribute]
        reveal_type(a_and_b.x)  # revealed: P & Q

        # error: [possibly-unbound-attribute]
        a_and_b.x = R()
    # Same for class objects
    def inner3_class(a_and_b: Intersection[type[A3], type[B3]]):
        # error: [possibly-unbound-attribute]
        reveal_type(a_and_b.x)  # revealed: P & Q

        # error: [possibly-unbound-attribute]
        a_and_b.x = R()

    class A4: ...
    class B4: ...

    def inner4(a_and_b: Intersection[A4, B4]):
        # error: [unresolved-attribute]
        reveal_type(a_and_b.x)  # revealed: Unknown

        # error: [invalid-assignment]
        a_and_b.x = R()
    # Same for class objects
    def inner4_class(a_and_b: Intersection[type[A4], type[B4]]):
        # error: [unresolved-attribute]
        reveal_type(a_and_b.x)  # revealed: Unknown

        # error: [invalid-assignment]
        a_and_b.x = R()
```

### Intersection of implicit instance attributes

```py
from ty_extensions import Intersection

class P: ...
class Q: ...

class A:
    def __init__(self):
        self.x: P = P()

class B:
    def __init__(self):
        self.x: Q = Q()

def _(a_and_b: Intersection[A, B]):
    reveal_type(a_and_b.x)  # revealed: P & Q
```

## Attribute access on `Any`

The union of the set of types that `Any` could materialise to is equivalent to `object`. It follows
from this that attribute access on `Any` resolves to `Any` if the attribute does not exist on
`object` -- but if the attribute *does* exist on `object`, the type of the attribute is
`<type as it exists on object> & Any`.

```py
from typing import Any

class Foo(Any): ...

reveal_type(Foo.bar)  # revealed: Any
reveal_type(Foo.__repr__)  # revealed: (def __repr__(self) -> str) & Any
```

Similar principles apply if `Any` appears in the middle of an inheritance hierarchy:

```py
from typing import ClassVar, Literal

class A:
    x: ClassVar[Literal[1]] = 1

class B(Any): ...
class C(B, A): ...

reveal_type(C.__mro__)  # revealed: tuple[<class 'C'>, <class 'B'>, Any, <class 'A'>, <class 'object'>]
reveal_type(C.x)  # revealed: Literal[1] & Any
```

## Classes with custom `__getattr__` methods

### Basic

If a type provides a custom `__getattr__` method, we use the return type of that method as the type
for unknown attributes. Consider the following `CustomGetAttr` class:

```py
from typing import Literal

def flag() -> bool:
    return True

class GetAttrReturnType: ...

class CustomGetAttr:
    class_attr: int = 1

    if flag():
        possibly_unbound: bytes = b"a"

    def __init__(self) -> None:
        self.instance_attr: str = "a"

    def __getattr__(self, name: str) -> GetAttrReturnType:
        return GetAttrReturnType()
```

We can access arbitrary attributes on instances of this class, and the type of the attribute will be
`GetAttrReturnType`:

```py
c = CustomGetAttr()

reveal_type(c.whatever)  # revealed: GetAttrReturnType
```

If an attribute is defined on the class, it takes precedence over the `__getattr__` method:

```py
reveal_type(c.class_attr)  # revealed: int
```

If the class attribute is possibly unbound, we union the type of the attribute with the fallback
type of the `__getattr__` method:

```py
reveal_type(c.possibly_unbound)  # revealed: bytes | GetAttrReturnType
```

Instance attributes also take precedence over the `__getattr__` method:

```py
# Note: we could attempt to union with the fallback type of `__getattr__` here, as we currently do not
# attempt to determine if instance attributes are always bound or not. Neither mypy nor pyright do this,
# so it's not a priority.
reveal_type(c.instance_attr)  # revealed: str
```

Importantly, `__getattr__` is only called if attributes are accessed on instances, not if they are
accessed on the class itself:

```py
# error: [unresolved-attribute]
CustomGetAttr.whatever
```

### Type of the `name` parameter

If the `name` parameter of the `__getattr__` method is annotated with a (union of) literal type(s),
we only consider the attribute access to be valid if the accessed attribute is one of them:

```py
from typing import Literal

class Date:
    def __getattr__(self, name: Literal["day", "month", "year"]) -> int:
        return 0

date = Date()

reveal_type(date.day)  # revealed: int
reveal_type(date.month)  # revealed: int
reveal_type(date.year)  # revealed: int

# error: [unresolved-attribute] "Type `Date` has no attribute `century`"
reveal_type(date.century)  # revealed: Unknown
```

### `argparse.Namespace`

A standard library example of a class with a custom `__getattr__` method is `argparse.Namespace`:

```py
import argparse

def _(ns: argparse.Namespace):
    reveal_type(ns.whatever)  # revealed: Any
```

## Classes with custom `__setattr__` methods

### Basic

If a type provides a custom `__setattr__` method, we use the parameter type of that method as the
type to validate attribute assignments. Consider the following `CustomSetAttr` class:

```py
class CustomSetAttr:
    def __setattr__(self, name: str, value: int) -> None:
        pass
```

We can set arbitrary attributes on instances of this class:

```py
c = CustomSetAttr()

c.whatever = 42
```

### Type of the `name` parameter

If the `name` parameter of the `__setattr__` method is annotated with a (union of) literal type(s),
we only consider the attribute assignment to be valid if the assigned attribute is one of them:

```py
from typing import Literal

class Date:
    def __setattr__(self, name: Literal["day", "month", "year"], value: int) -> None:
        pass

date = Date()
date.day = 8
date.month = 4
date.year = 2025

# error: [unresolved-attribute] "Can not assign object of `Literal["UTC"]` to attribute `tz` on type `Date` with custom `__setattr__` method."
date.tz = "UTC"
```

### `argparse.Namespace`

A standard library example of a class with a custom `__setattr__` method is `argparse.Namespace`:

```py
import argparse

def _(ns: argparse.Namespace):
    ns.whatever = 42
```

## Objects of all types have a `__class__` method

The type of `x.__class__` is the same as `x`'s meta-type. `x.__class__` is always the same value as
`type(x)`.

```py
import typing_extensions

reveal_type(typing_extensions.__class__)  # revealed: <class 'ModuleType'>
reveal_type(type(typing_extensions))  # revealed: <class 'ModuleType'>

a = 42
reveal_type(a.__class__)  # revealed: <class 'int'>
reveal_type(type(a))  # revealed: <class 'int'>

b = "42"
reveal_type(b.__class__)  # revealed: <class 'str'>

c = b"42"
reveal_type(c.__class__)  # revealed: <class 'bytes'>

d = True
reveal_type(d.__class__)  # revealed: <class 'bool'>

e = (42, 42)
reveal_type(e.__class__)  # revealed: <class 'tuple'>

def f(a: int, b: typing_extensions.LiteralString, c: int | str, d: type[str]):
    reveal_type(a.__class__)  # revealed: type[int]
    reveal_type(type(a))  # revealed: type[int]

    reveal_type(b.__class__)  # revealed: <class 'str'>
    reveal_type(type(b))  # revealed: <class 'str'>

    reveal_type(c.__class__)  # revealed: type[int] | type[str]
    reveal_type(type(c))  # revealed: type[int] | type[str]

    # `type[type]`, a.k.a., either the class `type` or some subclass of `type`.
    # It would be incorrect to infer `Literal[type]` here,
    # as `c` could be some subclass of `str` with a custom metaclass.
    # All we know is that the metaclass must be a (non-strict) subclass of `type`.
    reveal_type(d.__class__)  # revealed: type[type]

reveal_type(f.__class__)  # revealed: <class 'FunctionType'>

class Foo: ...

reveal_type(Foo.__class__)  # revealed: <class 'type'>
```

## Module attributes

### Basic

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

### Nested module attributes

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

reveal_type(f.__defaults__)  # revealed: tuple[Any, ...] | None
reveal_type(f.__kwdefaults__)  # revealed: dict[str, Any] | None
```

Some attributes are special-cased, however:

```py
reveal_type(f.__get__)  # revealed: <method-wrapper `__get__` of `f`>
reveal_type(f.__call__)  # revealed: <method-wrapper `__call__` of `f`>
```

### Int-literal attributes

Most attribute accesses on int-literal types are delegated to `builtins.int`, since all literal
integers are instances of that class:

```py
reveal_type((2).bit_length)  # revealed: bound method Literal[2].bit_length() -> int
reveal_type((2).denominator)  # revealed: Literal[1]
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
# revealed: Overload[(value: bool, /) -> bool, (value: int, /) -> int]
reveal_type(True.__and__)
# revealed: Overload[(value: bool, /) -> bool, (value: int, /) -> int]
reveal_type(False.__or__)
```

Some attributes are special-cased, however:

```py
reveal_type(True.numerator)  # revealed: Literal[1]
reveal_type(False.real)  # revealed: Literal[0]
```

### Bytes-literal attributes

All attribute access on literal `bytes` types is currently delegated to `builtins.bytes`:

```py
# revealed: bound method Literal[b"foo"].join(iterable_of_bytes: Iterable[@Todo(Support for `typing.TypeAlias`)], /) -> bytes
reveal_type(b"foo".join)
# revealed: bound method Literal[b"foo"].endswith(suffix: @Todo(Support for `typing.TypeAlias`) | tuple[@Todo(Support for `typing.TypeAlias`), ...], start: SupportsIndex | None = ellipsis, end: SupportsIndex | None = ellipsis, /) -> bool
reveal_type(b"foo".endswith)
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

### Accessing attributes on `Never`

Arbitrary attributes can be accessed on `Never` without emitting any errors:

```py
from typing_extensions import Never

def f(never: Never):
    reveal_type(never.arbitrary_attribute)  # revealed: Never

    # Assigning `Never` to an attribute on `Never` is also allowed:
    never.another_attribute = never
```

### Cyclic implicit attributes

Inferring types for undeclared implicit attributes can be cyclic:

```py
class C:
    def __init__(self):
        self.x = 1

    def copy(self, other: "C"):
        self.x = other.x

reveal_type(C().x)  # revealed: Unknown | Literal[1]
```

If the only assignment to a name is cyclic, we just infer `Unknown` for that attribute:

```py
class D:
    def copy(self, other: "D"):
        self.x = other.x

reveal_type(D().x)  # revealed: Unknown
```

If there is an annotation for a name, we don't try to infer any type from the RHS of assignments to
that name, so these cases don't trigger any cycle:

```py
class E:
    def __init__(self):
        self.x: int = 1

    def copy(self, other: "E"):
        self.x = other.x

reveal_type(E().x)  # revealed: int

class F:
    def __init__(self):
        self.x = 1

    def copy(self, other: "F"):
        self.x: int = other.x

reveal_type(F().x)  # revealed: int

class G:
    def copy(self, other: "G"):
        self.x: int = other.x

reveal_type(G().x)  # revealed: int
```

We can even handle cycles involving multiple classes:

```py
class A:
    def __init__(self):
        self.x = 1

    def copy(self, other: "B"):
        self.x = other.x

class B:
    def copy(self, other: "A"):
        self.x = other.x

reveal_type(B().x)  # revealed: Unknown | Literal[1]
reveal_type(A().x)  # revealed: Unknown | Literal[1]
```

This case additionally tests our union/intersection simplification logic:

```py
class H:
    def __init__(self):
        self.x = 1

    def copy(self, other: "H"):
        self.x = other.x or self.x
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
# TODO: revealed: slice[Any, Literal[1], Any]
reveal_type(C.a_slice)  # revealed: slice[Any, Any, Any]
reveal_type(C.a_type)  # revealed: type
reveal_type(C.a_none)  # revealed: None
```

### Generic methods

We also detect implicit instance attributes on methods that are themselves generic. We have an extra
test for this because generic functions have an extra type-params scope in between the function body
scope and the outer scope, so we need to make sure that our implementation can still recognize `f`
as a method of `C` here:

```toml
[environment]
python-version = "3.12"
```

```py
class C:
    def f[T](self, t: T) -> T:
        self.x: int = 1
        return t

reveal_type(C().x)  # revealed: int
```

## Enum classes

Enums are not supported yet; attribute access on an enum class is inferred as `Todo`.

```py
import enum

reveal_type(enum.Enum.__members__)  # revealed: @Todo(Attribute access on enum classes)

class Foo(enum.Enum):
    BAR = 1

reveal_type(Foo.BAR)  # revealed: @Todo(Attribute access on enum classes)
reveal_type(Foo.BAR.value)  # revealed: @Todo(Attribute access on enum classes)
reveal_type(Foo.__members__)  # revealed: @Todo(Attribute access on enum classes)
```

## References

Some of the tests in the *Class and instance variables* section draw inspiration from
[pyright's documentation] on this topic.

[descriptor protocol tests]: descriptor_protocol.md
[pyright's documentation]: https://microsoft.github.io/pyright/#/type-concepts-advanced?id=class-and-instance-variables
[typing spec on `classvar`]: https://typing.python.org/en/latest/spec/class-compat.html#classvar
[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
