# Class attributes assignment

## Union of attributes

```py
if flag:
    class C:
        x = 1
else:
    class C:
        x = 2

y = C.x
reveal_type(y)  # revealed: Literal[1, 2]
```
