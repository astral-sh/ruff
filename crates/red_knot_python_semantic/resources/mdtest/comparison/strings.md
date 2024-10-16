# Comparing strings

## String literals

```py
def str_instance() -> str: ...
a = "abc" == "abc"
b = "ab_cd" <= "ab_ce"
c = "abc" in "ab cd"
d = "" not in "hello"
e = "--" is "--"
f = "A" is "B"
g = "--" is not "--"
h = "A" is not "B"
i = str_instance() < "..."
# ensure we're not comparing the interned salsa symbols, which compare by order of declaration.
j = "ab" < "ab_cd"

reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal[True]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[False]
reveal_type(e)  # revealed: bool
reveal_type(f)  # revealed: Literal[False]
reveal_type(g)  # revealed: bool
reveal_type(h)  # revealed: Literal[True]
reveal_type(i)  # revealed: bool
reveal_type(j)  # revealed: Literal[True]
```
