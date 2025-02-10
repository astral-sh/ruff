# Custom binary operations

## Class instances

```py
from typing import Literal

class Yes:
    def __add__(self, other) -> Literal["+"]:
        return "+"

    def __sub__(self, other) -> Literal["-"]:
        return "-"

    def __mul__(self, other) -> Literal["*"]:
        return "*"

    def __matmul__(self, other) -> Literal["@"]:
        return "@"

    def __truediv__(self, other) -> Literal["/"]:
        return "/"

    def __mod__(self, other) -> Literal["%"]:
        return "%"

    def __pow__(self, other) -> Literal["**"]:
        return "**"

    def __lshift__(self, other) -> Literal["<<"]:
        return "<<"

    def __rshift__(self, other) -> Literal[">>"]:
        return ">>"

    def __or__(self, other) -> Literal["|"]:
        return "|"

    def __xor__(self, other) -> Literal["^"]:
        return "^"

    def __and__(self, other) -> Literal["&"]:
        return "&"

    def __floordiv__(self, other) -> Literal["//"]:
        return "//"

class Sub(Yes): ...
class No: ...

# Yes implements all of the dunder methods.
reveal_type(Yes() + Yes())  # revealed: Literal["+"]
reveal_type(Yes() - Yes())  # revealed: Literal["-"]
reveal_type(Yes() * Yes())  # revealed: Literal["*"]
reveal_type(Yes() @ Yes())  # revealed: Literal["@"]
reveal_type(Yes() / Yes())  # revealed: Literal["/"]
reveal_type(Yes() % Yes())  # revealed: Literal["%"]
reveal_type(Yes() ** Yes())  # revealed: Literal["**"]
reveal_type(Yes() << Yes())  # revealed: Literal["<<"]
reveal_type(Yes() >> Yes())  # revealed: Literal[">>"]
reveal_type(Yes() | Yes())  # revealed: Literal["|"]
reveal_type(Yes() ^ Yes())  # revealed: Literal["^"]
reveal_type(Yes() & Yes())  # revealed: Literal["&"]
reveal_type(Yes() // Yes())  # revealed: Literal["//"]

# Sub inherits Yes's implementation of the dunder methods.
reveal_type(Sub() + Sub())  # revealed: Literal["+"]
reveal_type(Sub() - Sub())  # revealed: Literal["-"]
reveal_type(Sub() * Sub())  # revealed: Literal["*"]
reveal_type(Sub() @ Sub())  # revealed: Literal["@"]
reveal_type(Sub() / Sub())  # revealed: Literal["/"]
reveal_type(Sub() % Sub())  # revealed: Literal["%"]
reveal_type(Sub() ** Sub())  # revealed: Literal["**"]
reveal_type(Sub() << Sub())  # revealed: Literal["<<"]
reveal_type(Sub() >> Sub())  # revealed: Literal[">>"]
reveal_type(Sub() | Sub())  # revealed: Literal["|"]
reveal_type(Sub() ^ Sub())  # revealed: Literal["^"]
reveal_type(Sub() & Sub())  # revealed: Literal["&"]
reveal_type(Sub() // Sub())  # revealed: Literal["//"]

# No does not implement any of the dunder methods.
# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `No` and `No`"
reveal_type(No() + No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `-` is unsupported between objects of type `No` and `No`"
reveal_type(No() - No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `*` is unsupported between objects of type `No` and `No`"
reveal_type(No() * No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `@` is unsupported between objects of type `No` and `No`"
reveal_type(No() @ No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `/` is unsupported between objects of type `No` and `No`"
reveal_type(No() / No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `%` is unsupported between objects of type `No` and `No`"
reveal_type(No() % No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `**` is unsupported between objects of type `No` and `No`"
reveal_type(No() ** No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `<<` is unsupported between objects of type `No` and `No`"
reveal_type(No() << No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `>>` is unsupported between objects of type `No` and `No`"
reveal_type(No() >> No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `|` is unsupported between objects of type `No` and `No`"
reveal_type(No() | No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `^` is unsupported between objects of type `No` and `No`"
reveal_type(No() ^ No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `&` is unsupported between objects of type `No` and `No`"
reveal_type(No() & No())  # revealed: Unknown
# error: [unsupported-operator] "Operator `//` is unsupported between objects of type `No` and `No`"
reveal_type(No() // No())  # revealed: Unknown

# Yes does not implement any of the reflected dunder methods.
# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() + Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `-` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() - Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `*` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() * Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `@` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() @ Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `/` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() / Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `%` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() % Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `**` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() ** Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `<<` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() << Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `>>` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() >> Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `|` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() | Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `^` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() ^ Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `&` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() & Yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `//` is unsupported between objects of type `No` and `Yes`"
reveal_type(No() // Yes())  # revealed: Unknown
```

## Subclass reflections override superclass dunders

```py
from typing import Literal

class Yes:
    def __add__(self, other) -> Literal["+"]:
        return "+"

    def __sub__(self, other) -> Literal["-"]:
        return "-"

    def __mul__(self, other) -> Literal["*"]:
        return "*"

    def __matmul__(self, other) -> Literal["@"]:
        return "@"

    def __truediv__(self, other) -> Literal["/"]:
        return "/"

    def __mod__(self, other) -> Literal["%"]:
        return "%"

    def __pow__(self, other) -> Literal["**"]:
        return "**"

    def __lshift__(self, other) -> Literal["<<"]:
        return "<<"

    def __rshift__(self, other) -> Literal[">>"]:
        return ">>"

    def __or__(self, other) -> Literal["|"]:
        return "|"

    def __xor__(self, other) -> Literal["^"]:
        return "^"

    def __and__(self, other) -> Literal["&"]:
        return "&"

    def __floordiv__(self, other) -> Literal["//"]:
        return "//"

class Sub(Yes):
    def __radd__(self, other) -> Literal["r+"]:
        return "r+"

    def __rsub__(self, other) -> Literal["r-"]:
        return "r-"

    def __rmul__(self, other) -> Literal["r*"]:
        return "r*"

    def __rmatmul__(self, other) -> Literal["r@"]:
        return "r@"

    def __rtruediv__(self, other) -> Literal["r/"]:
        return "r/"

    def __rmod__(self, other) -> Literal["r%"]:
        return "r%"

    def __rpow__(self, other) -> Literal["r**"]:
        return "r**"

    def __rlshift__(self, other) -> Literal["r<<"]:
        return "r<<"

    def __rrshift__(self, other) -> Literal["r>>"]:
        return "r>>"

    def __ror__(self, other) -> Literal["r|"]:
        return "r|"

    def __rxor__(self, other) -> Literal["r^"]:
        return "r^"

    def __rand__(self, other) -> Literal["r&"]:
        return "r&"

    def __rfloordiv__(self, other) -> Literal["r//"]:
        return "r//"

class No:
    def __radd__(self, other) -> Literal["r+"]:
        return "r+"

    def __rsub__(self, other) -> Literal["r-"]:
        return "r-"

    def __rmul__(self, other) -> Literal["r*"]:
        return "r*"

    def __rmatmul__(self, other) -> Literal["r@"]:
        return "r@"

    def __rtruediv__(self, other) -> Literal["r/"]:
        return "r/"

    def __rmod__(self, other) -> Literal["r%"]:
        return "r%"

    def __rpow__(self, other) -> Literal["r**"]:
        return "r**"

    def __rlshift__(self, other) -> Literal["r<<"]:
        return "r<<"

    def __rrshift__(self, other) -> Literal["r>>"]:
        return "r>>"

    def __ror__(self, other) -> Literal["r|"]:
        return "r|"

    def __rxor__(self, other) -> Literal["r^"]:
        return "r^"

    def __rand__(self, other) -> Literal["r&"]:
        return "r&"

    def __rfloordiv__(self, other) -> Literal["r//"]:
        return "r//"

# Subclass reflected dunder methods take precedence over the superclass's regular dunders.
reveal_type(Yes() + Sub())  # revealed: Literal["r+"]
reveal_type(Yes() - Sub())  # revealed: Literal["r-"]
reveal_type(Yes() * Sub())  # revealed: Literal["r*"]
reveal_type(Yes() @ Sub())  # revealed: Literal["r@"]
reveal_type(Yes() / Sub())  # revealed: Literal["r/"]
reveal_type(Yes() % Sub())  # revealed: Literal["r%"]
reveal_type(Yes() ** Sub())  # revealed: Literal["r**"]
reveal_type(Yes() << Sub())  # revealed: Literal["r<<"]
reveal_type(Yes() >> Sub())  # revealed: Literal["r>>"]
reveal_type(Yes() | Sub())  # revealed: Literal["r|"]
reveal_type(Yes() ^ Sub())  # revealed: Literal["r^"]
reveal_type(Yes() & Sub())  # revealed: Literal["r&"]
reveal_type(Yes() // Sub())  # revealed: Literal["r//"]

# But for an unrelated class, the superclass regular dunders are used.
reveal_type(Yes() + No())  # revealed: Literal["+"]
reveal_type(Yes() - No())  # revealed: Literal["-"]
reveal_type(Yes() * No())  # revealed: Literal["*"]
reveal_type(Yes() @ No())  # revealed: Literal["@"]
reveal_type(Yes() / No())  # revealed: Literal["/"]
reveal_type(Yes() % No())  # revealed: Literal["%"]
reveal_type(Yes() ** No())  # revealed: Literal["**"]
reveal_type(Yes() << No())  # revealed: Literal["<<"]
reveal_type(Yes() >> No())  # revealed: Literal[">>"]
reveal_type(Yes() | No())  # revealed: Literal["|"]
reveal_type(Yes() ^ No())  # revealed: Literal["^"]
reveal_type(Yes() & No())  # revealed: Literal["&"]
reveal_type(Yes() // No())  # revealed: Literal["//"]
```

## Classes

Dunder methods defined in a class are available to instances of that class, but not to the class
itself. (For these operators to work on the class itself, they would have to be defined on the
class's type, i.e. `type`.)

```py
from typing import Literal

class Yes:
    def __add__(self, other) -> Literal["+"]:
        return "+"

class Sub(Yes): ...
class No: ...

# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `Literal[Yes]` and `Literal[Yes]`"
reveal_type(Yes + Yes)  # revealed: Unknown
# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `Literal[Sub]` and `Literal[Sub]`"
reveal_type(Sub + Sub)  # revealed: Unknown
# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `Literal[No]` and `Literal[No]`"
reveal_type(No + No)  # revealed: Unknown
```

## Subclass

```py
from typing import Literal

class Yes:
    def __add__(self, other) -> Literal["+"]:
        return "+"

class Sub(Yes): ...
class No: ...

def yes() -> type[Yes]:
    return Yes

def sub() -> type[Sub]:
    return Sub

def no() -> type[No]:
    return No

# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `type[Yes]` and `type[Yes]`"
reveal_type(yes() + yes())  # revealed: Unknown
# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `type[Sub]` and `type[Sub]`"
reveal_type(sub() + sub())  # revealed: Unknown
# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `type[No]` and `type[No]`"
reveal_type(no() + no())  # revealed: Unknown
```

## Function literals

```py
def f():
    pass

# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f + f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `-` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f - f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `*` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f * f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `@` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f @ f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `/` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f / f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `%` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f % f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `**` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f**f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `<<` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f << f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>>` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f >> f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `|` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f | f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `^` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f ^ f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `&` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f & f)  # revealed: Unknown
# error: [unsupported-operator] "Operator `//` is unsupported between objects of type `Literal[f]` and `Literal[f]`"
reveal_type(f // f)  # revealed: Unknown
```
