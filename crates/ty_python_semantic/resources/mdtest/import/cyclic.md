## Cyclic imports

### Regression tests

#### Issue 261

See: <https://github.com/astral-sh/ty/issues/261>

`main.py`:

```py
from foo import bar

reveal_type(bar)  # revealed: <module 'foo.bar'>
```

`foo/__init__.py`:

```py
from foo import bar

__all__ = ["bar"]
```

`foo/bar/__init__.py`:

```py
# empty
```

#### Issue 113

See: <https://github.com/astral-sh/ty/issues/113>

`main.py`:

```py
from pkg.sub import A

# TODO: This should be `<class 'A'>`
reveal_type(A)  # revealed: Divergent
```

`pkg/outer.py`:

```py
class A: ...
```

`pkg/sub/__init__.py`:

```py
from ..outer import *
from .inner import *
```

`pkg/sub/inner.py`:

```py
from pkg.sub import A
```

#### Issue 3812

Cycle recovery should merge different specializations of the same generic class inside the
specialization. Treating the two approximations as a union of class objects creates an unsupported
base and makes the inherited `rank` attribute dynamic.

See: <https://github.com/astral-sh/ty/issues/3812>

`steam/__init__.py`:

```py
```

`steam/abc.py`:

```py
from typing import TYPE_CHECKING, Generic, Protocol

from typing_extensions import TypeVar

if TYPE_CHECKING:
    from .message import Group

UserT = TypeVar("UserT", default=object)
MessageT = TypeVar("MessageT", bound="Message", default="Message")

class PartialUser: ...
class Messageable(Protocol[MessageT]): ...

GroupT = TypeVar("GroupT", default="Group")

class Channel(Messageable[MessageT], Generic[MessageT, GroupT]): ...

ChannelT = TypeVar("ChannelT", default=Channel)

class Message(Generic[UserT, ChannelT]): ...
```

`steam/chat.py`:

```py
from typing import TYPE_CHECKING, Generic

from typing_extensions import TypeVar

from .abc import Channel, GroupT, Message, Messageable, PartialUser

if TYPE_CHECKING:
    from .message import GroupMessage, UserMessage

ChatT = TypeVar("ChatT", bound="Chat", default="Chat", covariant=True)
MemberT = TypeVar("MemberT", default=PartialUser)
AuthorT = TypeVar("AuthorT")

class ProtoMember:
    def __init__(self, rank: str) -> None: ...

class WrapsUser(PartialUser, Messageable["UserMessage"]): ...  # error: [invalid-type-arguments]

class PartialMember(PartialUser):
    rank: int

class Member(WrapsUser, PartialMember, Generic[MemberT]):
    def copy(self) -> None:
        ProtoMember(self.rank)  # error: [invalid-argument-type]

class ChatMessage(Message[AuthorT, ChatT], Generic[AuthorT, MemberT, ChatT]): ...  # error: [invalid-generic-class]

ChatMessageT = TypeVar(
    "ChatMessageT",
    bound="GroupMessage",
    default="GroupMessage",
)

class Chat(Channel[ChatMessageT, GroupT]): ...  # error: [invalid-type-arguments]
class ChatGroup(Generic[MemberT, ChatT]): ...
```

`steam/message.py`:

```py
from typing_extensions import TypeVar

from .chat import ChatGroup, ChatMessage, Member, PartialMember

class GroupMember(Member): ...
class Group(ChatGroup): ...
class UserMessage: ...

GroupMessageAuthorT = TypeVar("GroupMessageAuthorT", bound=PartialMember, default=PartialMember)

class GroupMessage(ChatMessage[GroupMessageAuthorT, GroupMember]): ...
```

### Actual cycle

The following example fails at runtime. Ideally, we would emit a diagnostic here. For now, we only
make sure that this does not lead to a module resolution cycle.

`main.py`:

```py
from module import x

reveal_type(x)  # revealed: Unknown
```

`module.py`:

```py
# error: [unresolved-import]
from module import x
```

### Self-referential `from` import in a nested scope

A `from <self> import <name>` inside a function body should resolve the name from the module's
global scope without triggering a cycle.

See: <https://github.com/astral-sh/ty/issues/2596>

`main.py`:

```py
def foo() -> int:
    return 0

def bar() -> int:
    from main import foo

    return foo()
```

### Normal self-referential import

Some modules like `sys` in typeshed import themselves. Here, we make sure that this does not lead to
cycles or unresolved imports.

`module/__init__.py`:

```py
import module  # self-referential import

from module.sub import x
```

`module/sub.py`:

```py
x: int = 1
```

`main.py`:

```py
from module import x

reveal_type(x)  # revealed: int
```
