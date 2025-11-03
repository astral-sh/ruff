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
