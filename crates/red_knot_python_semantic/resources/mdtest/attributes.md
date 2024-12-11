# Class attributes

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

## Inherited attributes

```py
class A:
    X = "foo"

class B(A): ...
class C(B): ...

reveal_type(C.X)  # revealed: Literal["foo"]
```

## Inherited attributes (multiple inheritance)

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

## Unions with all paths unbound

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
