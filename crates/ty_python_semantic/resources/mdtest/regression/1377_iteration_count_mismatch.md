# Iteration count mismatch for highly cyclic type vars

Regression test for <https://github.com/astral-sh/ty/issues/1377>.

The code is an excerpt from <https://github.com/Gobot1234/steam.py>.

```toml
[environment]
extra-paths= ["/packages"]
```

`main.py`:

```py
from __future__ import annotations

from typing import TypeAlias

from steam.message import Message

TestAlias: TypeAlias = tuple[Message]
```

`/packages/steam/__init__.py`:

```py

```

`/packages/steam/abc.py`:

```py
from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Generic, Protocol

from typing_extensions import TypeVar

if TYPE_CHECKING:
    from .clan import Clan
    from .group import Group

UserT = TypeVar("UserT", covariant=True)
MessageT = TypeVar("MessageT", bound="Message", default="Message", covariant=True)

class Messageable(Protocol[MessageT]): ...

ClanT = TypeVar("ClanT", bound="Clan | None", default="Clan | None", covariant=True)
GroupT = TypeVar("GroupT", bound="Group | None", default="Group | None", covariant=True)

class Channel(Messageable[MessageT], Generic[MessageT, ClanT, GroupT]): ...

ChannelT = TypeVar("ChannelT", bound=Channel, default=Channel, covariant=True)

class Message(Generic[UserT, ChannelT]): ...
```

`/packages/steam/chat.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING, Generic, TypeAlias

from typing_extensions import Self, TypeVar

from .abc import Channel, ClanT, GroupT, Message

if TYPE_CHECKING:
    from .clan import Clan
    from .message import ClanMessage, GroupMessage

ChatT = TypeVar("ChatT", bound="Chat", default="Chat", covariant=True)
MemberT = TypeVar("MemberT", covariant=True)

AuthorT = TypeVar("AuthorT", covariant=True)

class ChatMessage(Message[AuthorT, ChatT], Generic[AuthorT, MemberT, ChatT]): ...

ChatMessageT = TypeVar("ChatMessageT", bound="GroupMessage | ClanMessage", default="GroupMessage | ClanMessage", covariant=True)

class Chat(Channel[ChatMessageT, ClanT, GroupT]): ...

ChatGroupTypeT = TypeVar("ChatGroupTypeT", covariant=True)

class ChatGroup(Generic[MemberT, ChatT, ChatGroupTypeT]): ...
```

`/packages/steam/channel.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING, Any

from .chat import Chat

if TYPE_CHECKING:
    from .clan import Clan

class ClanChannel(Chat["Clan", None]): ...
```

`/packages/steam/clan.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING, TypeVar

from typing_extensions import Self

from .chat import ChatGroup

class Clan(ChatGroup[str], str): ...
```

`/packages/steam/group.py`:

```py
from __future__ import annotations

from .chat import ChatGroup

class Group(ChatGroup[str]): ...
```

`/packages/steam/message.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING

from typing_extensions import TypeVar

from .abc import BaseUser, Message
from .chat import ChatMessage

if TYPE_CHECKING:
    from .channel import ClanChannel

class GroupMessage(ChatMessage["str"]): ...
class ClanMessage(ChatMessage["ClanChannel"]): ...
```
