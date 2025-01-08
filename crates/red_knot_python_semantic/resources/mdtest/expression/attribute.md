# Attribute access

## Boundness

```py
def _(flag: bool):
    class A:
        always_bound = 1

        if flag:
            union = 1
        else:
            union = "abc"

        if flag:
            possibly_unbound = "abc"

    reveal_type(A.always_bound)  # revealed: Literal[1]

    reveal_type(A.union)  # revealed: Literal[1, "abc"]

    # error: [possibly-unbound-attribute] "Attribute `possibly_unbound` on type `Literal[A]` is possibly unbound"
    reveal_type(A.possibly_unbound)  # revealed: Literal["abc"]

    # error: [unresolved-attribute] "Type `Literal[A]` has no attribute `non_existent`"
    reveal_type(A.non_existent)  # revealed: Unknown
```
