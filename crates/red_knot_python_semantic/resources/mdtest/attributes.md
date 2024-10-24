# Class attributes

## Union of attributes

```py
if flag:
    class C:
        x = 1

else:
    class C:
        x = 2

reveal_type(C.x)  # revealed: Literal[1, 2]
```
