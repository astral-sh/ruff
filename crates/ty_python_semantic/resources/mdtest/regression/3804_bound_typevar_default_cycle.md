# Regression test for #3804

Regression test for [this issue](https://github.com/astral-sh/ty/issues/3804).

`cog.py`:

```py
from commands import GroupMixin

class Group(GroupMixin): ...
class Bot(GroupMixin): ...
```

`commands.py`:

```py
from typing import TYPE_CHECKING, Generic
from typing_extensions import TypeVar

if TYPE_CHECKING:
    from cog import Bot

CogT = TypeVar("CogT", bound="Bot", default="Bot", covariant=True)

class GroupMixin(Generic[CogT]):
    def method(self): ...

def call_method(value: object):
    if isinstance(value, GroupMixin):
        value.method()
```
