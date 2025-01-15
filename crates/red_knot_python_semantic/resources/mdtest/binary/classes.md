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

```py
class A: ...
class B: ...

# error: "Operator `|` is unsupported between objects of type `Literal[A]` and `Literal[B]`"
reveal_type(A | B)  # revealed: Unknown
```
