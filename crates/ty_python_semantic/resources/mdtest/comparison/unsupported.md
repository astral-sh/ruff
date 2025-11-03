# Comparison: Unsupported operators

```py
def _(flag: bool, flag1: bool, flag2: bool):
    class A: ...
    a = 1 in 7  # error: "Operator `in` is not supported for types `Literal[1]` and `Literal[7]`"
    reveal_type(a)  # revealed: bool

    b = 0 not in 10  # error: "Operator `not in` is not supported for types `Literal[0]` and `Literal[10]`"
    reveal_type(b)  # revealed: bool

    # error: [unsupported-operator] "Operator `<` is not supported for types `object` and `int`, in comparing `object` with `Literal[5]`"
    c = object() < 5
    reveal_type(c)  # revealed: Unknown

    # error: [unsupported-operator] "Operator `<` is not supported for types `int` and `object`, in comparing `Literal[5]` with `object`"
    d = 5 < object()
    reveal_type(d)  # revealed: Unknown

    int_literal_or_str_literal = 1 if flag else "foo"
    # error: "Operator `in` is not supported for types `Literal[42]` and `Literal[1]`, in comparing `Literal[42]` with `Literal[1, "foo"]`"
    e = 42 in int_literal_or_str_literal
    reveal_type(e)  # revealed: bool

    # error: [unsupported-operator] "Operator `<` is not supported for types `int` and `str`, in comparing `tuple[Literal[1], Literal[2]]` with `tuple[Literal[1], Literal["hello"]]`"
    f = (1, 2) < (1, "hello")
    reveal_type(f)  # revealed: Unknown

    # error: [unsupported-operator] "Operator `<` is not supported for types `A` and `A`, in comparing `tuple[bool, A]` with `tuple[bool, A]`"
    g = (flag1, A()) < (flag2, A())
    reveal_type(g)  # revealed: Unknown
```
