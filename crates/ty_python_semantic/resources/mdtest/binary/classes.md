# Binary operations on classes

## Union of two classes

Unioning two classes via the `|` operator is only available in Python 3.10 and later.

```toml
[environment]
python-version = "3.10"
```

```py
class A: ...
class B: ...

reveal_type(A | B)  # revealed: UnionType
```

## Union of two classes (prior to 3.10)

```toml
[environment]
python-version = "3.9"
```

```py
class A: ...
class B: ...

# error: "Operator `|` is unsupported between objects of type `<class 'A'>` and `<class 'B'>`"
reveal_type(A | B)  # revealed: Unknown
```

## Other binary operations resulting in `UnionType`

```toml
[environment]
python-version = "3.12"
```

```py
class A: ...
class B: ...

def _(sub_a: type[A], sub_b: type[B]):
    reveal_type(A | sub_b)  # revealed: UnionType
    reveal_type(sub_a | B)  # revealed: UnionType
    reveal_type(sub_a | sub_b)  # revealed: UnionType

class C[T]: ...
class D[T]: ...

reveal_type(C | D)  # revealed: UnionType

reveal_type(C[int] | D[str])  # revealed: UnionType
```
