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

    reveal_type(A.union)  # revealed: Unknown | Literal[1, "abc"]

    reveal_type(A.union_declared)  # revealed: int | str

    # error: [possibly-unbound-attribute] "Attribute `possibly_unbound` on type `Literal[A]` is possibly unbound"
    reveal_type(A.possibly_unbound)  # revealed: str

    # error: [unresolved-attribute] "Type `Literal[A]` has no attribute `non_existent`"
    reveal_type(A.non_existent)  # revealed: Unknown
```
