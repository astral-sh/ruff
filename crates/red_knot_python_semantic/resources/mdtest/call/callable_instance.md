# Callable instance

## Dunder call

```py
class Multiplier:
    def __init__(self, factor: float):
        self.factor = factor

    def __call__(self, number: float) -> float:
        return number * self.factor

a = Multiplier(2.0)(3.0)
reveal_type(a)  # revealed: float

class Unit: ...

b = Unit()(3.0)  # error: "Object of type `Unit` is not callable"
reveal_type(b)  # revealed: Unknown
```
