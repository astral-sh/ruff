# Mro tests

## No bases

```py
class C:
    pass

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[object]]
```

## The special case: `object` itself

```py
reveal_type(object.__mro__)  # revealed: tuple[Literal[object]]
```

## Explicit inheritance from `object`

```py
class C(object):
    pass

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[object]]
```

## Explicit inheritance from non-`object` single base

```py
class A:
    pass

class B(A):
    pass

reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[A], Literal[object]]
```
