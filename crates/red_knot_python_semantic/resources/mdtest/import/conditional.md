# Conditional imports

## Reimport

```py path=c.py
def f(): ...
```

```py path=b.py
if flag:
    from c import f
else:
    def f(): ...
```

```py
# TODO we should not emit this error
from b import f # error: [invalid-assignment] "Object of type `Literal[f, f]` is not assignable to `Literal[f, f]`"
# TODO: We should disambiguate in such cases, showing `Literal[b.f, c.f]`.
reveal_type(f)  # revealed: Literal[f, f]
```

## Reimport with stub declaration

When we have a declared type in one path and only an inferred-from-definition type in the other, we
should still be able to unify those:

```py path=c.pyi
x: int
```

```py path=b.py
if flag:
    from c import x
else:
    x = 1
```

```py
from b import x
reveal_type(x)  # revealed: int
```
