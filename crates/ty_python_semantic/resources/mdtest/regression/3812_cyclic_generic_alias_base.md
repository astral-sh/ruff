# Cyclic generic alias base

Regression test for <https://github.com/astral-sh/ty/issues/3812>.

`steam/__init__.py`:

```py
```

`steam/chat.py`:

```py
from typing import TYPE_CHECKING, Protocol

from typing_extensions import TypeVar

if TYPE_CHECKING:
    from .message import UserMessage

T = TypeVar("T")

class Messageable(Protocol[T]): ...
class WrapsUser(Messageable["UserMessage"]): ...

class PartialMember:
    rank: int

class Member(WrapsUser, PartialMember): ...

reveal_type(Member().rank)  # revealed: int
```

`steam/message.py`:

```py
from .chat import Member

class GroupMember(Member): ...
class UserMessage: ...
```
