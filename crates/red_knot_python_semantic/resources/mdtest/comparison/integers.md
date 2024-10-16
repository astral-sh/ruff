# Comparing integers

## Integer literals

```py
a = 1 == 1 == True
b = 1 == 1 == 2 == 4
c = False < True <= 2 < 3 != 6
d = 1 < 1
e = 1 > 1
f = 1 is 1
g = 1 is not 1
h = 1 is 2
i = 1 is not 7
j = 1 <= "" and 0 < 1

reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal[False]
reveal_type(c)  # revealed: Literal[True]
reveal_type(d)  # revealed: Literal[False]
reveal_type(e)  # revealed: Literal[False]
reveal_type(f)  # revealed: bool
reveal_type(g)  # revealed: bool
reveal_type(h)  # revealed: Literal[False]
reveal_type(i)  # revealed: Literal[True]
reveal_type(j)  # revealed: @Todo | Literal[True]
```

## Integer instance

```py
# TODO: implement lookup of `__eq__` on typeshed `int` stub.
def int_instance() -> int: ...
a = 1 == int_instance()
b = 9 < int_instance()
c = int_instance() < int_instance()

reveal_type(a)  # revealed: @Todo
reveal_type(b)  # revealed: bool
reveal_type(c)  # revealed: bool
```
