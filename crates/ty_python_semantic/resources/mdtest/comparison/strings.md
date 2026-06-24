# Comparison: Strings

## String literals

```py
def _(x: str):
    reveal_type("abc" == "abc")  # revealed: Literal[True]
    reveal_type("ab_cd" <= "ab_ce")  # revealed: Literal[True]
    reveal_type("abc" in "ab cd")  # revealed: Literal[False]
    reveal_type("" not in "hello")  # revealed: Literal[False]
    reveal_type("--" is "--")  # revealed: bool
    reveal_type("A" is "B")  # revealed: Literal[False]
    reveal_type("--" is not "--")  # revealed: bool
    reveal_type("A" is not "B")  # revealed: Literal[True]
    reveal_type(x < "...")  # revealed: bool

    # ensure we're not comparing the interned salsa symbols, which compare by order of declaration.
    reveal_type("ab" < "ab_cd")  # revealed: Literal[True]
```
