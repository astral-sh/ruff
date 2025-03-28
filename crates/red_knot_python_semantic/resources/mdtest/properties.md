# Properties

## Basic

```py
class C:
    @property
    def f(self) -> int:
        return 1

reveal_type(C.f)  # revealed: property

reveal_type(type(C.f))  # revealed: Literal[property]

reveal_type(type(C.f).__get__)  # revealed: <wrapper-descriptor `__get__` of `property` objects>

reveal_type(C().f)  # revealed: int
```
