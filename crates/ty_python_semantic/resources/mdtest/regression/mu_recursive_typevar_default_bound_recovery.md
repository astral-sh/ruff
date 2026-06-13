# Recursive TypeVar default and bound recovery

```toml
[environment]
python-version = "3.13"
```

This is a reduced regression test from steam.py. Lazy TypeVar defaults and bounds can form a
cycle through generic class bases; recovering those queries must not re-enter unrelated definition
inference while checking whether generic classes have defaults, whether a class is a TypedDict, or
whether a decorator preserves a class binding.

`steam/__init__.py`:

```py
```

`steam/abc.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING, Generic, Protocol

from typing_extensions import TypeVar

if TYPE_CHECKING:
    from .clan import Clan
    from .group import Group

UserT = TypeVar("UserT", covariant=True)
# error: [missing-type-argument]
# error: [missing-type-argument]
MessageT = TypeVar("MessageT", bound="Message", default="Message", covariant=True)
IDT = TypeVar("IDT", bound=int, default=int, covariant=True)


class BaseUser: ...


class ID(Generic[IDT]): ...


class Messageable(Protocol[MessageT]): ...


ClanT = TypeVar("ClanT", bound="Clan | None", default="Clan | None", covariant=True)
GroupT = TypeVar("GroupT", bound="Group | None", default="Group | None", covariant=True)


class Channel(Messageable[MessageT], Generic[MessageT, ClanT, GroupT]): ...


ChannelT = TypeVar("ChannelT", bound=Channel, default=Channel, covariant=True)


class Message(Generic[UserT, ChannelT]): ...
```

`steam/chat.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING, Generic, Literal

from typing_extensions import TypeVar

from .abc import Channel, ClanT, GroupT, ID, Message

if TYPE_CHECKING:
    from .clan import Clan
    from .message import ClanMessage, GroupMessage

ChatT = TypeVar("ChatT", bound="Chat", default="Chat", covariant=True)
MemberT = TypeVar("MemberT", bound="Member", default="Member", covariant=True)
AuthorT = TypeVar("AuthorT", bound="PartialMember", default="PartialMember | Member", covariant=True)


RegisteredT = TypeVar("RegisteredT", bound=type[object])


class PartialMember:
    @classmethod
    def register(cls, member: RegisteredT) -> RegisteredT:
        return member


@PartialMember.register
class Member(PartialMember, Generic[ClanT, GroupT]): ...


class ChatMessage(Message[AuthorT, ChatT], Generic[AuthorT, MemberT, ChatT]): ...


ChatMessageT = TypeVar(
    "ChatMessageT",
    bound="GroupMessage | ClanMessage",
    default="GroupMessage | ClanMessage",
    covariant=True,
)


class Chat(Channel[ChatMessageT, ClanT, GroupT]): ...


ChatGroupTypeT = TypeVar(
    "ChatGroupTypeT",
    bound=Literal[1, 2],
    default=Literal[1, 2],
    covariant=True,
)


class ChatGroup(ID[ChatGroupTypeT], Generic[MemberT, ChatT, ChatGroupTypeT]): ...
```

`steam/channel.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING

from .abc import Channel
from .chat import Chat
from .message import ClanMessage, GroupMessage, UserMessage

if TYPE_CHECKING:
    from .clan import Clan
    from .group import Group


class UserChannel(Channel["UserMessage", None, None]): ...
class GroupChannel(Chat[GroupMessage, None, "Group"]): ...
class ClanChannel(Chat[ClanMessage, "Clan", None]): ...
```

`steam/message.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING

from typing_extensions import TypeVar

from .abc import BaseUser
from .chat import ChatMessage

if TYPE_CHECKING:
    from .channel import ClanChannel
    from .clan import ClanMember
    from .group import GroupMember


GroupMessageAuthorT = TypeVar(
    # error: [invalid-type-variable-default]
    "GroupMessageAuthorT", bound="BaseUser", default="BaseUser | GroupMember", covariant=True
)

ClanMessageAuthorT = TypeVar(
    "ClanMessageAuthorT", bound="BaseUser", default="BaseUser | ClanMember", covariant=True
)


# error: [invalid-type-arguments]
class ClanMessage(ChatMessage[ClanMessageAuthorT, "ClanMember", "ClanChannel"]): ...
# error: [invalid-type-arguments]
# error: [unresolved-reference]
class GroupMessage(ChatMessage[GroupMessageAuthorT, "GroupMember", "GroupChannel"]): ...
# error: [invalid-type-arguments]
class UserMessage(ChatMessage[str]): ...
```

`steam/clan.py`:

```py
from __future__ import annotations

from typing import Literal

from .channel import ClanChannel
from .chat import ChatGroup, Member


# error: [unsupported-base]
class ClanMember(Member["Clan", None]): ...


class Clan(ChatGroup[ClanMember, ClanChannel, Literal[1]], str): ...
```

`steam/group.py`:

```py
from __future__ import annotations

from typing import Literal

from .channel import GroupChannel
from .chat import ChatGroup, Member


class GroupMember(Member[None, "Group"]): ...


class Group(ChatGroup[GroupMember, GroupChannel, Literal[2]]): ...
```

`main.py`:

```py
from steam.channel import ClanChannel, GroupChannel
```
