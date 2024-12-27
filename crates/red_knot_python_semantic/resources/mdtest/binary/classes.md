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
