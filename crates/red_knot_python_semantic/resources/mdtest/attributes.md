# Attributes

Tests for attribute access on various kinds of types.

## Class and instance variables

### Pure instance variables

#### Variable only declared/defined in `__init__`

Variables only defined in `__init__` are pure instance variables. They can not be accessed on the
class itself.

```py
class C:
    def __init__(self, value2: int) -> None:
        # bound but not declared
        self.pure_instance_variable1 = "value set in __init__"

        # bound but not declared - with type inferred from parameter
        self.pure_instance_variable2 = value2

        # declared but not bound
        self.pure_instance_variable3: bytes

        # declared and bound
        self.pure_instance_variable4: bool = True

c_instance = C(1)

# TODO: should be `Literal["value set in __init__"]` (or `str` which would probably be more generally useful)
reveal_type(c_instance.pure_instance_variable1)  # revealed: @Todo(instance attributes)

# TODO: should be `int`
reveal_type(c_instance.pure_instance_variable2)  # revealed: @Todo(instance attributes)

# TODO: should be `bytes`
reveal_type(c_instance.pure_instance_variable3)  # revealed: @Todo(instance attributes)

# TODO: should be `Literal[True]` (or `bool`)
reveal_type(c_instance.pure_instance_variable4)  # revealed: @Todo(instance attributes)

c_instance.pure_instance_variable1 = "value set on instance"

# TODO: this should be an error (incompatible types in assignment)
c_instance.pure_instance_variable2 = "incompatible"

# TODO: we already show an error here but the message might be improved?
# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
# error: [unresolved-attribute] "Type `Literal[C]` has no attribute `pure_instance_variable1`"
reveal_type(C.pure_instance_variable1)  # revealed: Unknown

# TODO: this should be an error (pure instance variables can not be accessed on the class)
# mypy shows no error here, but pyright raises "reportAttributeAccessIssue"
C.pure_instance_variable1 = "overwritten on class"

# TODO: should ideally be `Literal["value set on instance"]`
reveal_type(c_instance.pure_instance_variable1)  # revealed: @Todo(instance attributes)
```

#### Variable declared in class body and defined in `__init__`

The same rule applies even if the variable is *declared* (not defined!) in the class body: it is
still a pure instance variable.

```py
class C:
    pure_instance_variable: str

    def __init__(self) -> None:
        self.pure_instance_variable = "value set in __init__"

c_instance = C()

# TODO: should be `Literal["value set in __init__"]` (or `str` which would probably be more generally useful)
reveal_type(c_instance.pure_instance_variable)  # revealed: @Todo(instance attributes)

# TODO: we currently plan to emit a diagnostic here. Note that both mypy
# and pyright show no error in this case! So we may reconsider this in
# the future, if it turns out to produce too many false positives.
reveal_type(C.pure_instance_variable)  # revealed: str

# TODO: same as above. We plan to emit a diagnostic here, even if both mypy
# and pyright allow this.
C.pure_instance_variable = "overwritten on class"

# TODO: this should be an error (incompatible types in assignment)
c_instance.pure_instance_variable = 1
```

#### Variable declared in class body and defined in unrelated method

We also recognize pure instance variables if they are defined in a method that is not `__init__`.

```py
class C:
    pure_instance_variable: str

    def set_instance_variable(self) -> None:
        self.pure_instance_variable = "value set in method"

c_instance = C()

# for a more realistic example, let's actually call the method
c_instance.set_instance_variable()

# TODO: should be `str`
reveal_type(c_instance.pure_instance_variable)  # revealed: @Todo(instance attributes)

# TODO: See above, we currently plan to emit diagnostics for both of these lines,
# even if mypy/pyright do not.
reveal_type(C.pure_instance_variable)  # revealed: str
C.pure_instance_variable = "overwritten on class"
```

### Pure class variables (`ClassVar`)

#### Annotated with `ClassVar` type qualifier

Class variables annotated with the [`typing.ClassVar`] type qualifier are pure class variables. They
can not be accessed on instances.

```py
from typing import ClassVar

class C:
    pure_class_variable: ClassVar[str] = "value in class body"

reveal_type(C.pure_class_variable)  # revealed: str

c_instance = C()

# TODO: This should be `str`. It is okay to access a pure class variable on an instance.
reveal_type(c_instance.pure_class_variable)  # revealed: @Todo(instance attributes)

# TODO: should raise an error. It is not allowed to reassign a pure class variable on an instance.
c_instance.pure_class_variable = "value set on instance"

C.pure_class_variable = "overwritten on class"

# TODO: should ideally be `Literal["overwritten on class"]`, but not a priority
reveal_type(C.pure_class_variable)  # revealed: str

# TODO: should raise an error (incompatible types in assignment)
C.pure_class_variable = 1
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

# TODO: mypy shows an error here, pyright does not. What should we do?
# error: [unresolved-attribute]
reveal_type(C.pure_class_variable)  # revealed: Unknown

C.pure_class_variable = "overwritten on class"

# TODO: should be `Literal["overwritten on class"]`
# error: [unresolved-attribute]
reveal_type(C.pure_class_variable)  # revealed: Unknown

c_instance = C()
# TODO: should be `Literal["overwritten on class"]` or `str`
reveal_type(c_instance.pure_class_variable)  # revealed: @Todo(instance attributes)

# TODO: should raise an error.
c_instance.pure_class_variable = "value set on instance"
```

### "Regular" class variables

These are instance attributes, but the fact that we can see that they have a binding (not a
declaration) in the class body means that reading the value from the class body is also permitted.
This is the only difference for these attributes as opposed to "pure" instance attributes.

#### Basic

```py
class C:
    regular_class_variable: str = "value in class body"

    def instance_method(self):
        self.regular_class_variable = "value set in instance method"

reveal_type(C.regular_class_variable)  # revealed: str

c_instance = C()

# TODO: should be `str`
reveal_type(c_instance.regular_class_variable)  # revealed: @Todo(instance attributes)

c_instance.regular_class_variable = "value set on instance"

reveal_type(C.regular_class_variable)  # revealed: str

# TODO: should ideally be Literal["value set on instance"], or still `str`
reveal_type(c_instance.regular_class_variable)  # revealed: @Todo(instance attributes)

C.regular_class_variable = "overwritten on class"

# TODO: should ideally be `Literal["overwritten on class"]`
reveal_type(C.regular_class_variable)  # revealed: str

# TODO: should still be `Literal["value set on instance"]`
reveal_type(c_instance.regular_class_variable)  # revealed: @Todo(instance attributes)
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

    reveal_type(C1.x)  # revealed: Literal[1, 2]
    reveal_type(C2.x)  # revealed: Literal[3, 4]
```

## Inherited class attributes

### Basic

```py
class A:
    X = "foo"

class B(A): ...
class C(B): ...

reveal_type(C.X)  # revealed: Literal["foo"]
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
reveal_type(A.X)  # revealed: Literal[42]
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
    reveal_type(C.x)  # revealed: Literal[1, 3]
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
    reveal_type(C.x)  # revealed: Literal[1, 2, 3]
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

## Literal types

### Function-literal attributes

Most attribute accesses on function-literal types are delegated to `types.FunctionType`, since all
functions are instances of that class:

```py path=a.py
def f(): ...

reveal_type(f.__defaults__)  # revealed: @Todo(instance attributes)
reveal_type(f.__kwdefaults__)  # revealed: @Todo(instance attributes)
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
reveal_type((2).bit_length)  # revealed: @Todo(instance attributes)
reveal_type((2).denominator)  # revealed: @Todo(instance attributes)
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
reveal_type(True.__and__)  # revealed: @Todo(instance attributes)
reveal_type(False.__or__)  # revealed: @Todo(instance attributes)
```

Some attributes are special-cased, however:

```py path=b.py
reveal_type(True.numerator)  # revealed: Literal[1]
reveal_type(False.real)  # revealed: Literal[0]
```

### Bytes-literal attributes

All attribute access on literal `bytes` types is currently delegated to `buitins.bytes`:

```py
reveal_type(b"foo".join)  # revealed: @Todo(instance attributes)
reveal_type(b"foo".endswith)  # revealed: @Todo(instance attributes)
```

## References

Some of the tests in the *Class and instance variables* section draw inspiration from
[pyright's documentation] on this topic.

[pyright's documentation]: https://microsoft.github.io/pyright/#/type-concepts-advanced?id=class-and-instance-variables
[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
