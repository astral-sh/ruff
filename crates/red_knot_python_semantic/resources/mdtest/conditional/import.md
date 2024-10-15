# Conditional imports

## Reimport

In cases where a module conditionally imports symbols from another module or provides its own definition, the type system should be able to infer the correct type from the relevant branch. However, disambiguation may still be required for complex cases.

TODO: We should disambiguate in such cases, showing `Literal[b.f, c.f]`.

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
from b import f # error: [invalid-assignment] "Object of type `Literal[f, f]` is not assignable to `Literal[f, f]`"
reveal_type(f)  # revealed: Literal[f, f]
```

## Reimport with stub declaration

When a conditional import involves both an import from another module and a local definition, the system correctly infers the type from the declared type of the imported symbol.

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
