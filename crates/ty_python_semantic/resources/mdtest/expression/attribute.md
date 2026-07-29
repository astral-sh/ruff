# Attribute access

## Boundness

```py
def _(flag: bool):
    class A:
        always_bound: int = 1

        if flag:
            union = 1
        else:
            union = "abc"

        if flag:
            union_declared: int = 1
        else:
            union_declared: str = "abc"

        if flag:
            possibly_unbound: str = "abc"

    reveal_type(A.always_bound)  # revealed: int

    reveal_type(A.union)  # revealed: int | str

    reveal_type(A.union_declared)  # revealed: int | str

    # error: [possibly-missing-attribute] "Attribute `possibly_unbound` may be missing on class `A`"
    reveal_type(A.possibly_unbound)  # revealed: str

    # error: [unresolved-attribute] "Class `A` has no attribute `non_existent`"
    reveal_type(A.non_existent)  # revealed: Unknown
```

## Walrus attribute access after later rebinding

```py
class IntBox:
    attr: int

class StrBox:
    attr: str

def f() -> None:
    (box := IntBox()).attr = 1
    box = StrBox()
    reveal_type(box.attr)  # revealed: str
```

## Local prefixes block enclosing whole-place bindings

An enclosing binding for an entire member access refers to a different object when the nested scope
binds the root locally. The local object's member type takes precedence.

```py
class OuterBox:
    attr: int

class InnerBox:
    attr: str

def outer_root() -> None:
    box = OuterBox()
    box.attr = 1

    def inner() -> None:
        box = InnerBox()
        reveal_type(box.attr)  # revealed: str
```

The same rule applies when an intermediate member, rather than the root, is bound in the nested
scope.

```py
class Holder:
    box: OuterBox | InnerBox

def outer_member() -> None:
    holder = Holder()
    holder.box = OuterBox()
    holder.box.attr = 1

    def inner() -> None:
        holder.box = InnerBox()
        reveal_type(holder.box.attr)  # revealed: str
```

If none of the prefixes are bound in the nested scope, the enclosing whole-place binding remains
visible.

```py
def outer_fallback() -> None:
    box = OuterBox()
    box.attr = 1

    def inner() -> None:
        reveal_type(box.attr)  # revealed: int
```
