# Comparison: Enums

```py
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

reveal_type(Answer.NO == Answer.NO)  # revealed: Literal[True]
reveal_type(Answer.NO == Answer.YES)  # revealed: Literal[False]

reveal_type(Answer.NO != Answer.NO)  # revealed: Literal[False]
reveal_type(Answer.NO != Answer.YES)  # revealed: Literal[True]

reveal_type(Answer.NO is Answer.NO)  # revealed: Literal[True]
reveal_type(Answer.NO is Answer.YES)  # revealed: Literal[False]

reveal_type(Answer.NO is not Answer.NO)  # revealed: Literal[False]
reveal_type(Answer.NO is not Answer.YES)  # revealed: Literal[True]
```

Equality result inference uses the same runtime semantics as equality narrowing. This allows us to
recognize when two enum domains cannot contain equal values, even if neither operand is a singleton:

```py
from enum import Enum
from typing import Literal

class Choice(str, Enum):
    FIRST = "first"
    SECOND = "second"
    THIRD = "third"
    FOURTH = "fourth"

def compare(
    left: Literal[Choice.FIRST, Choice.SECOND],
    right: Literal[Choice.THIRD, Choice.FOURTH],
):
    reveal_type(left == right)  # revealed: Literal[False]
    reveal_type(left != right)  # revealed: Literal[True]
```

Scalar enum members from different classes compare according to their underlying values:

```py
from enum import Enum

class X(str, Enum):
    A = "a"
    B = "b"

class Y(str, Enum):
    A = "a"
    B = "b"

reveal_type(X.A == Y.A)  # revealed: Literal[True]
reveal_type(X.A != Y.A)  # revealed: Literal[False]
reveal_type(X.A == Y.B)  # revealed: Literal[False]
```

When equality semantics are custom, result inference falls back to the corresponding dunder method:

```py
from enum import Enum

class EqResult: ...
class NeResult: ...

class CustomEquality(Enum):
    MEMBER = 1

    def __eq__(self, other: object) -> EqResult:  # error: [invalid-method-override]
        return EqResult()

    def __ne__(self, other: object) -> NeResult:  # error: [invalid-method-override]
        return NeResult()

reveal_type(CustomEquality.MEMBER == CustomEquality.MEMBER)  # revealed: EqResult
reveal_type(CustomEquality.MEMBER != CustomEquality.MEMBER)  # revealed: NeResult
```
