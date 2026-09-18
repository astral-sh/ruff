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

When a nested function binds the root name of a whole-place access, the local root binding takes
precedence over the root binding from the enclosing scope.

```py
class IntBox:
    attr: int

class StrBox:
    attr: str

box = IntBox()

def outer_root() -> None:
    box.attr = 1

    def inner() -> None:
        box = StrBox()
        reveal_type(box.attr)  # revealed: str
```

Similarly, a nested rebinding of an intermediate member, rather than the root, takes precedence over
the enclosing binding of that same intermediate member.

```py
class Holder:
    box: IntBox | StrBox

def outer_member() -> None:
    holder = Holder()
    holder.box = IntBox()
    holder.box.attr = 1

    def inner() -> None:
        holder.box = StrBox()
        reveal_type(holder.box.attr)  # revealed: str
```

Under Python's function name-resolution rules, even a conditional assignment to the root name makes
the local root take precedence over the enclosing binding. When the condition is false, the unbound
local root cannot fall back to the binding in the enclosing scope.

```py
def with_inner_conditional_root(flag: bool) -> None:
    box = IntBox()
    box.attr = 1

    def inner() -> None:
        if flag:
            box = StrBox()
        # error: [possibly-unresolved-reference] "Name `box` used when possibly not defined"
        reveal_type(box.attr)  # revealed: str
```

By contrast, binding an intermediate member does not affect resolution of the root, which still
comes from the enclosing scope. When the intermediate member is conditionally rebound, it can refer
to either object, so both member types remain visible in the whole-place access.

```py
def with_inner_conditional_member(flag: bool) -> None:
    holder = Holder()
    holder.box = IntBox()
    holder.box.attr = 1

    def inner() -> None:
        if flag:
            holder.box = StrBox()
        reveal_type(holder.box.attr)  # revealed: int | str
```

If none of the prefixes are bound in the nested scope, the enclosing whole-place binding remains
visible.

```py
def outer_fallback() -> None:
    box = IntBox()
    box.attr = 1

    def inner() -> None:
        reveal_type(box.attr)  # revealed: int
```
