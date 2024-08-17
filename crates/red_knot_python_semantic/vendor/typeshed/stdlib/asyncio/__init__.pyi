import sys
from collections.abc import Awaitable, Coroutine, Generator
from typing import Any, TypeVar
from typing_extensions import TypeAlias

# As at runtime, this depends on all submodules defining __all__ accurately.
from .base_events import *
from .coroutines import *
from .events import *
from .exceptions import *
from .futures import *
from .locks import *
from .protocols import *
from .queues import *
from .runners import *
from .streams import *
from .subprocess import *
from .tasks import *
from .transports import *

if sys.version_info >= (3, 9):
    from .threads import *

if sys.version_info >= (3, 11):
    from .taskgroups import *
    from .timeouts import *

if sys.platform == "win32":
    from .windows_events import *
else:
    from .unix_events import *

_T_co = TypeVar("_T_co", covariant=True)

# Aliases imported by multiple submodules in typeshed
if sys.version_info >= (3, 12):
    _AwaitableLike: TypeAlias = Awaitable[_T_co]  # noqa: Y047
    _CoroutineLike: TypeAlias = Coroutine[Any, Any, _T_co]  # noqa: Y047
else:
    _AwaitableLike: TypeAlias = Generator[Any, None, _T_co] | Awaitable[_T_co]
    _CoroutineLike: TypeAlias = Generator[Any, None, _T_co] | Coroutine[Any, Any, _T_co]
