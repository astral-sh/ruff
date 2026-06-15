# Recursive TypeVar defaults through dataclass channel recovery

This is reduced from steam.py. Checking `steam/channel.py` used to panic because cycle recovery for definition inference constructed a divergent overlay, called the full semantic single-valued check for an unknown nominal instance, and re-entered lazy TypeVar default evaluation through enum metadata and class MRO queries.

```toml
[environment]
python-version = "3.13"
```

`steam/__init__.py`:

```py

```

`steam/abc.py`:

```py
from __future__ import annotations

import abc
from collections.abc import AsyncGenerator, Coroutine
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Generic, Literal, Protocol, cast

from typing_extensions import TypeVar

if TYPE_CHECKING:
    from .clan import Clan
    from .group import Group

UserT = TypeVar("UserT", covariant=True)
# error: [missing-type-argument]
# error: [missing-type-argument]
MessageT = TypeVar("MessageT", bound="Message", default="Message", covariant=True)
IDT = TypeVar("IDT", bound=int, default=int, covariant=True)


class ID(Generic[IDT]): ...


class Commentable(Protocol): ...


class PartialUser(ID[Literal[1]], Commentable): ...


class BaseUser(PartialUser): ...


class Messageable(Protocol[MessageT]):
    _state: object

    @abc.abstractmethod
    def _send_message(self, content: str) -> Coroutine[Any, Any, MessageT]: ...

    @abc.abstractmethod
    async def history(self) -> AsyncGenerator[MessageT, None]:
        yield cast(MessageT, None)

    async def fetch_message(self, id: int) -> MessageT | None:
        async for message in self.history():
            return message
        return None


ClanT = TypeVar("ClanT", bound="Clan | None", default="Clan | None", covariant=True)
GroupT = TypeVar("GroupT", bound="Group | None", default="Group | None", covariant=True)


@dataclass(slots=True)
class Channel(Messageable[MessageT], Generic[MessageT, ClanT, GroupT]):
    _state: object
    clan: ClanT = cast(ClanT, None)
    group: GroupT = cast(GroupT, None)


ChannelT = TypeVar("ChannelT", bound=Channel, default=Channel, covariant=True)


class Message(Generic[UserT, ChannelT], metaclass=abc.ABCMeta):
    __slots__ = ("author", "channel", "group", "clan")

    author: UserT

    def __init__(self, channel: ChannelT, proto: Any):
        self.channel = channel
        self.group = channel.group
        self.clan = channel.clan

    @classmethod
    @abc.abstractmethod
    def _from_history(cls, channel: ChannelT, proto: Any): ...
```

`steam/chat.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING, Any, Generic, Literal, Protocol, TypeAlias, cast, runtime_checkable

from typing_extensions import Self, TypeVar

from .abc import Channel, ClanT, GroupT, ID, Message, PartialUser

if TYPE_CHECKING:
    from .clan import Clan
    from .group import Group
    from .message import ClanMessage, GroupMessage


class _HasChatGroupMixin(Protocol): ...
class WrapsUser: ...
class IncomingChatMessageNotification: ...
class State: ...
class ChatRoomState: ...


ChatT = TypeVar("ChatT", bound="Chat", default="Chat", covariant=True)
# error: [invalid-type-form]
# error: [invalid-type-form]
MemberT = TypeVar("MemberT", bound="Member", default="Member", covariant=True)


@runtime_checkable
class _PartialMemberProto(_HasChatGroupMixin, Protocol):
    clan: Clan | None
    group: Group | None


class PartialMember(PartialUser, _PartialMemberProto):
    clan: Clan | None
    group: Group | None

    @classmethod
    def register(cls, member: type[object]) -> type[object]:
        return member


if TYPE_CHECKING:

    class _BaseMember(WrapsUser, PartialMember, _PartialMemberProto):
        pass

else:
    _BaseMember = WrapsUser


@PartialMember.register
class Member(_BaseMember, _HasChatGroupMixin, Generic[ClanT, GroupT]):
    def __init__(self, state: object, chat_group: ChatGroup[Any, Any], user: object, member: object) -> None:
        self.clan = cast(ClanT, None)
        self.group = cast(GroupT, None)


# error: [invalid-type-form]
AuthorT = TypeVar("AuthorT", bound="PartialMember", default="PartialMember | Member", covariant=True)


class ChatMessage(Message[AuthorT, ChatT], Generic[AuthorT, MemberT, ChatT]):
    channel: ChatT
    author: AuthorT

    def __init__(self, proto: object, channel: ChatT, author: AuthorT) -> None:
        super().__init__(channel, proto)
        self.author = author


ChatMessageT = TypeVar(
    "ChatMessageT", bound="GroupMessage | ClanMessage", default="GroupMessage | ClanMessage", covariant=True
)

GroupChannelProtos: TypeAlias = IncomingChatMessageNotification | State | ChatRoomState


class Chat(Channel[ChatMessageT, ClanT, GroupT], _HasChatGroupMixin):
    def __init__(self, state: object, chat_group: ChatGroup[Any, Self], proto: GroupChannelProtos):
        super().__init__(state)

    async def fetch_message(self, id: int) -> ChatMessageT | None:
        message = await super().fetch_message(id)
        if message is not None:
            return message


ChatGroupTypeT = TypeVar(
    "ChatGroupTypeT", bound=Literal[1, 2], default=Literal[1, 2], covariant=True
)


class ChatGroup(ID[ChatGroupTypeT], Generic[MemberT, ChatT, ChatGroupTypeT]): ...
```

`steam/channel.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING

from .abc import Channel
from .chat import Chat, GroupChannelProtos
from .message import ClanMessage, GroupMessage, UserMessage

if TYPE_CHECKING:
    from .clan import Clan
    from .group import Group


class UserChannel(Channel["UserMessage", None, None]): ...


class GroupChannel(Chat[GroupMessage, None, "Group"]):
    def __init__(self, state: object, group: Group, proto: GroupChannelProtos):
        super().__init__(state, group, proto)
        self.group = group


class ClanChannel(Chat[ClanMessage, "Clan", None]):
    def __init__(self, state: object, clan: Clan, proto: GroupChannelProtos):
        super().__init__(state, clan, proto)
        self.clan = clan
```

`steam/message.py`:

```py
from __future__ import annotations

from typing import TYPE_CHECKING

from typing_extensions import TypeVar

from .abc import BaseUser
from .chat import ChatMessage, PartialMember

if TYPE_CHECKING:
    from .channel import ClanChannel, GroupChannel, UserChannel
    from .clan import ClanMember
    from .group import GroupMember


GroupMessageAuthorT = TypeVar(
    "GroupMessageAuthorT", bound="PartialMember", default="PartialMember | GroupMember", covariant=True
)

ClanMessageAuthorT = TypeVar(
    "ClanMessageAuthorT", bound="PartialMember", default="PartialMember | ClanMember", covariant=True
)


class ClanMessage(ChatMessage[ClanMessageAuthorT, "ClanMember", "ClanChannel"]): ...
class GroupMessage(ChatMessage[GroupMessageAuthorT, "GroupMember", "GroupChannel"]): ...
class UserMessage(ChatMessage[PartialMember, PartialMember, "UserChannel"]): ...  # error: [invalid-type-arguments]
```

`steam/clan.py`:

```py
from __future__ import annotations

from typing import Literal

from .channel import ClanChannel
from .chat import ChatGroup, Member


class ClanMember(Member["Clan", None]): ...  # error: [not-subscriptable]


class Clan(ChatGroup[ClanMember, ClanChannel, Literal[1]], str): ...
```

`steam/group.py`:

```py
from __future__ import annotations

from typing import Literal

from .channel import GroupChannel
from .chat import ChatGroup, Member


class GroupMember(Member[None, "Group"]): ...  # error: [not-subscriptable]


class Group(ChatGroup[GroupMember, GroupChannel, Literal[2]]): ...
```
