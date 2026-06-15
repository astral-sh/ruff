# Recursive attribute cycle after `Never`

A previous version of a module can contain attribute operations on `Never`. After the module is
updated to contain implicit attribute cycles, checking should still terminate and infer the finite
attribute type.

## Accessing attributes on `Never`

```py
from typing_extensions import Never

def f(never: Never):
    reveal_type(never.arbitrary_attribute)  # revealed: Never
    never.another_attribute = never
```

## Many implicit attribute cycles

```py
class ManyCycles:
    def __init__(self: "ManyCycles"):
        self.x1 = 0
        self.x2 = 0
        self.x3 = 1

    def f1(self: "ManyCycles"):
        self.x1 = self.x2 + self.x3
        self.x2 = self.x1 + self.x3
        self.x3 = self.x1 + self.x2

    def f2(self: "ManyCycles"):
        self.x1 = self.x2 + self.x3
        self.x2 = self.x1 + self.x3
        self.x3 = self.x1 + self.x2

        reveal_type(self.x1)  # revealed: int
        reveal_type(self.x2)  # revealed: int
        reveal_type(self.x3)  # revealed: int
```
