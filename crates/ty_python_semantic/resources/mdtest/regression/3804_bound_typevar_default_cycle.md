# Regression test for #3804

Regression test for [this issue](https://github.com/astral-sh/ty/issues/3804).

```toml
[environment]
python-version = "3.13"
```

## Nominal default

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

## Type alias default

A valid type alias default must not be discarded just because its value is inferred as part of a
cycle.

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

type BotAlias = Bot

T = TypeVar("T", bound="Bot", default=BotAlias, covariant=True)

class GroupMixin(Generic[T]):
    def method(self): ...

def call_method(value: object):
    if isinstance(value, GroupMixin):
        value.method()
```
