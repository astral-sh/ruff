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
