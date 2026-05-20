# Binary operations on classes

## Union of two classes

Unioning two classes via the `|` operator:

```py
class A: ...
class B: ...

reveal_type(A | B)  # revealed: <types.UnionType special-form 'A | B'>
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
    reveal_type(A | sub_b)  # revealed: <types.UnionType special-form>
    reveal_type(sub_a | B)  # revealed: <types.UnionType special-form>
    reveal_type(sub_a | sub_b)  # revealed: <types.UnionType special-form>

class C[T]: ...
class D[T]: ...

reveal_type(C | D)  # revealed: <types.UnionType special-form 'C[Unknown] | D[Unknown]'>

reveal_type(C[int] | D[str])  # revealed: <types.UnionType special-form 'C[int] | D[str]'>
```
