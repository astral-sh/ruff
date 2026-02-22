# Comparison: Unsupported operators

<!-- snapshot-diagnostics -->

```py
def _(flag: bool, flag1: bool, flag2: bool):
    class A: ...
    a = 1 in 7  # error: "Operator `in` is not supported between objects of type `Literal[1]` and `Literal[7]`"
    reveal_type(a)  # revealed: bool

    b = 0 not in 10  # error: "Operator `not in` is not supported between objects of type `Literal[0]` and `Literal[10]`"
    reveal_type(b)  # revealed: bool

    # error: [unsupported-operator] "Operator `<` is not supported between objects of type `object` and `Literal[5]`"
    c = object() < 5
    reveal_type(c)  # revealed: Unknown

    # error: [unsupported-operator] "Operator `<` is not supported between objects of type `Literal[5]` and `object`"
    d = 5 < object()
    reveal_type(d)  # revealed: Unknown

    int_literal_or_str_literal = 1 if flag else "foo"
    # error: "Operator `in` is not supported between objects of type `Literal[42]` and `Literal[1, "foo"]`"
    e = 42 in int_literal_or_str_literal
    reveal_type(e)  # revealed: bool

    # error: [unsupported-operator] "Operator `<` is not supported between objects of type `tuple[Literal[1], Literal[2]]` and `tuple[Literal[1], Literal["hello"]]`"
    f = (1, 2) < (1, "hello")
    reveal_type(f)  # revealed: Unknown

    # error: [unsupported-operator] "Operator `<` is not supported between two objects of type `tuple[bool, A]`"
    g = (flag1, A()) < (flag2, A())
    reveal_type(g)  # revealed: Unknown
```
