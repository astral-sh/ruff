# Class attributes

## Union of attributes

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()

if flag:
    class C:
        x = 1

else:
    class C:
        x = 2

reveal_type(C.x)  # revealed: Literal[1, 2]
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
