# Unary Operations

```py
class Number:
    def __init__(self, value: int):
        self.value = 1

    def __pos__(self) -> int:
        return +self.value

    def __neg__(self) -> int:
        return -self.value

    def __invert__(self) -> int:
        return ~self.value

a = Number()

reveal_type(+a) # revealed: int
reveal_type(-a) # revealed: int
reveal_type(~a) # revealed: int

class NoDunder:
  ...

b = NoDunder()
+b
-b
~b
reveal_type(+b) # revealed: Unknown
reveal_type(-b) # revealed: Unknown
reveal_type(~b) # revealed: Unknown

```
