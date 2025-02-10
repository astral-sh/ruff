# Conditional imports

## Maybe unbound

`maybe_unbound.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    y = 3

x = y  # error: [possibly-unresolved-reference]

# revealed: Literal[3]
reveal_type(x)

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(y)
```

```py
# error: [possibly-unbound-import] "Member `y` of module `maybe_unbound` is possibly unbound"
from maybe_unbound import x, y

reveal_type(x)  # revealed: Unknown | Literal[3]
reveal_type(y)  # revealed: Unknown | Literal[3]
```

## Maybe unbound annotated

`maybe_unbound_annotated.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    y: int = 3

x = y  # error: [possibly-unresolved-reference]

# revealed: Literal[3]
reveal_type(x)

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(y)
```

Importing an annotated name prefers the declared type over the inferred type:

```py
# error: [possibly-unbound-import] "Member `y` of module `maybe_unbound_annotated` is possibly unbound"
from maybe_unbound_annotated import x, y

reveal_type(x)  # revealed: Unknown | Literal[3]
reveal_type(y)  # revealed: int
```

## Maybe undeclared

Importing a possibly undeclared name still gives us its declared type:

`maybe_undeclared.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    x: int
```

```py
from maybe_undeclared import x

reveal_type(x)  # revealed: int
```

## Reimport

`c.py`:

```py
def f(): ...
```

`b.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    from c import f
else:
    def f(): ...
```

```py
from b import f

# TODO: We should disambiguate in such cases, showing `Literal[b.f, c.f]`.
reveal_type(f)  # revealed: Literal[f, f]
```

## Reimport with stub declaration

When we have a declared type in one path and only an inferred-from-definition type in the other, we
should still be able to unify those:

`c.pyi`:

```pyi
x: int
```

`b.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    from c import x
else:
    x = 1
```

```py
from b import x

reveal_type(x)  # revealed: int
```
