# Class attributes

## Union of attributes

```py
def bool_instance() -> bool:
    return True

flag = bool_instance()

if flag:
    class C:
        x = 1

else:
    class C:
        x = 2

reveal_type(C.x)  # revealed: Literal[1, 2]
```
