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

# TODO: Mypy/pyright infer `int | str` here. We want this to be `Unknown | Literal[1, "a"]`
reveal_type(c_instance.inferred_from_value)  # revealed: @Todo(implicit instance attribute)

# TODO: Same here. This should be `Unknown | Literal[1, "a"]`
reveal_type(c_instance.inferred_from_other_attribute)  # revealed: @Todo(implicit instance attribute)

# TODO: should be `int | None`
reveal_type(c_instance.inferred_from_param)  # revealed: @Todo(implicit instance attribute)

# TODO: should be `bytes`
reveal_type(c_instance.declared_only)  # revealed: @Todo(implicit instance attribute)

# TODO: should be `bool`
reveal_type(c_instance.declared_and_bound)  # revealed: @Todo(implicit instance attribute)

# TODO: should be `str`
# We probably don't want to emit a diagnostic for this being possibly undeclared/unbound.
# mypy and pyright do not show an error here.
reveal_type(c_instance.possibly_undeclared_unbound)  # revealed: @Todo(implicit instance attribute)

# This assignment is fine, as we infer `Unknown | Literal[1, "a"]` for `inferred_from_value`.
c_instance.inferred_from_value = "value set on instance"

# This assignment is also fine:
c_instance.inferred_from_param = None

# TODO: this should be an error (incompatible types in assignment)
c_instance.inferred_from_param = "incompatible"

# TODO: we already show an error here but the message might be improved?
# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
# error: [unresolved-attribute] "Type `Literal[C]` has no attribute `inferred_from_value`"
reveal_type(C.inferred_from_value)  # revealed: Unknown

# TODO: this should be an error (pure instance variables cannot be accessed on the class)
# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
C.inferred_from_value = "overwritten on class"

# This assignment is fine:
c_instance.declared_and_bound = False

# TODO: After this assignment to the attribute within this scope, we may eventually want to narrow
# the `bool` type (see above) for this instance variable to `Literal[False]` here. This is unsound
# in general (we don't know what else happened to `c_instance` between the assignment and the use
# here), but mypy and pyright support this. In conclusion, this could be `bool` but should probably
# be `Literal[False]`.
reveal_type(c_instance.declared_and_bound)  # revealed: @Todo(implicit instance attribute)
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

# TODO: we currently plan to emit a diagnostic here. Note that both mypy
# and pyright show no error in this case! So we may reconsider this in
# the future, if it turns out to produce too many false positives.
reveal_type(C.declared_and_bound)  # revealed: str | None

# TODO: same as above. We plan to emit a diagnostic here, even if both mypy
# and pyright allow this.
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

# TODO: mypy and pyright do not show an error here, but we plan to emit a diagnostic.
# The type could be changed to 'Unknown' if we decide to emit an error?
reveal_type(C.only_declared)  # revealed: str

# TODO: mypy and pyright do not show an error here, but we plan to emit one.
C.only_declared = "overwritten on class"
```

#### Variable only defined in unrelated method

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

# TODO: Should be `Unknown | Literal[1, "a"]`
reveal_type(c_instance.inferred_from_value)  # revealed: @Todo(implicit instance attribute)

# TODO: Should be `Unknown | Literal[1, "a"]`
reveal_type(c_instance.inferred_from_other_attribute)  # revealed: @Todo(implicit instance attribute)

# TODO: Should be `int | None`
reveal_type(c_instance.inferred_from_param)  # revealed: @Todo(implicit instance attribute)

# TODO: Should be `bytes`
reveal_type(c_instance.declared_only)  # revealed: @Todo(implicit instance attribute)

# TODO: Should be `bool`
reveal_type(c_instance.declared_and_bound)  # revealed: @Todo(implicit instance attribute)

# TODO: We already show an error here, but the message might be improved?
# error: [unresolved-attribute]
reveal_type(C.inferred_from_value)  # revealed: Unknown

# TODO: this should be an error
C.inferred_from_value = "overwritten on class"
```

#### Methods that does not use `self` as a first parameter

```py
class C:
    # This might trigger a stylistic lint like `invalid-first-argument-name-for-method`, but
    # it should be supported in general:
    def __init__(this) -> None:
        this.declared_and_bound: str | None = "a"

# TODO: should be `str | None`
reveal_type(C().declared_and_bound)  # revealed: @Todo(implicit instance attribute)
```

#### Aliased `self` parameter

```py
class C:
    def __init__(self) -> None:
        this = self
        this.declared_and_bound: str | None = "a"

# TODO: This would ideally be `str | None`, but mypy/pyright don't support this either,
# so `Unknown` + a diagnostic is also fine.
reveal_type(C().declared_and_bound)  # revealed: @Todo(implicit instance attribute)
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

C.pure_class_variable = "overwritten on class"

# TODO: should be `Literal["overwritten on class"]`
# error: [unresolved-attribute]
reveal_type(C.pure_class_variable)  # revealed: Unknown

c_instance = C()
# TODO: should be `Literal["overwritten on class"]`
reveal_type(c_instance.pure_class_variable)  # revealed: @Todo(implicit instance attribute)

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

# TODO: Should be `str | None`
reveal_type(Derived().defined_in_init)  # revealed: @Todo(implicit instance attribute)
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

```py
import typing_extensions

reveal_type(typing_extensions.__class__)  # revealed: Literal[ModuleType]

a = 42
reveal_type(a.__class__)  # revealed: Literal[int]

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
    reveal_type(b.__class__)  # revealed: Literal[str]
    reveal_type(c.__class__)  # revealed: type[int] | type[str]

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

```py path=mod.py
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

```py path=outer/__init__.py
```

```py path=outer/nested/__init__.py
```

```py path=outer/nested/inner.py
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

```py path=a.py
def f(): ...

reveal_type(f.__defaults__)  # revealed: @Todo(full tuple[...] support) | None
reveal_type(f.__kwdefaults__)  # revealed: @Todo(generics) | None
```

Some attributes are special-cased, however:

```py path=b.py
def f(): ...

reveal_type(f.__get__)  # revealed: @Todo(`__get__` method on functions)
reveal_type(f.__call__)  # revealed: @Todo(`__call__` method on functions)
```

### Int-literal attributes

Most attribute accesses on int-literal types are delegated to `builtins.int`, since all literal
integers are instances of that class:

```py path=a.py
reveal_type((2).bit_length)  # revealed: @Todo(bound method)
reveal_type((2).denominator)  # revealed: @Todo(@property)
```

Some attributes are special-cased, however:

```py path=b.py
reveal_type((2).numerator)  # revealed: Literal[2]
reveal_type((2).real)  # revealed: Literal[2]
```

### Bool-literal attributes

Most attribute accesses on bool-literal types are delegated to `builtins.bool`, since all literal
bols are instances of that class:

```py path=a.py
reveal_type(True.__and__)  # revealed: @Todo(bound method)
reveal_type(False.__or__)  # revealed: @Todo(bound method)
```

Some attributes are special-cased, however:

```py path=b.py
reveal_type(True.numerator)  # revealed: Literal[1]
reveal_type(False.real)  # revealed: Literal[0]
```

### Bytes-literal attributes

All attribute access on literal `bytes` types is currently delegated to `buitins.bytes`:

```py
reveal_type(b"foo".join)  # revealed: @Todo(bound method)
reveal_type(b"foo".endswith)  # revealed: @Todo(bound method)
```

## References

Some of the tests in the *Class and instance variables* section draw inspiration from
[pyright's documentation] on this topic.

[pyright's documentation]: https://microsoft.github.io/pyright/#/type-concepts-advanced?id=class-and-instance-variables
[typing spec on `classvar`]: https://typing.readthedocs.io/en/latest/spec/class-compat.html#classvar
[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
