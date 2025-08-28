# This py312+ module provides annotations for `sys.monitoring`.
# It's named `sys._monitoring` in typeshed,
# because trying to import `sys.monitoring` will fail at runtime!
# At runtime, `sys.monitoring` has the unique status
# of being a `types.ModuleType` instance that cannot be directly imported,
# and exists in the `sys`-module namespace despite `sys` not being a package.

import sys
from collections.abc import Callable
from types import CodeType
from typing import Any, Final, type_check_only
from typing_extensions import deprecated

DEBUGGER_ID: Final = 0
COVERAGE_ID: Final = 1
PROFILER_ID: Final = 2
OPTIMIZER_ID: Final = 5

def use_tool_id(tool_id: int, name: str, /) -> None: ...
def free_tool_id(tool_id: int, /) -> None: ...
def get_tool(tool_id: int, /) -> str | None: ...

events: Final[_events]

@type_check_only
class _events:
    CALL: Final[int]
    C_RAISE: Final[int]
    C_RETURN: Final[int]
    EXCEPTION_HANDLED: Final[int]
    INSTRUCTION: Final[int]
    JUMP: Final[int]
    LINE: Final[int]
    NO_EVENTS: Final[int]
    PY_RESUME: Final[int]
    PY_RETURN: Final[int]
    PY_START: Final[int]
    PY_THROW: Final[int]
    PY_UNWIND: Final[int]
    PY_YIELD: Final[int]
    RAISE: Final[int]
    RERAISE: Final[int]
    STOP_ITERATION: Final[int]
    if sys.version_info >= (3, 14):
        BRANCH_LEFT: Final[int]
        BRANCH_TAKEN: Final[int]

        @property
        @deprecated("Deprecated since Python 3.14. Use `BRANCH_LEFT` or `BRANCH_TAKEN` instead.")
        def BRANCH(self) -> int: ...

    else:
        BRANCH: Final[int]

def get_events(tool_id: int, /) -> int: ...
def set_events(tool_id: int, event_set: int, /) -> None: ...
def get_local_events(tool_id: int, code: CodeType, /) -> int: ...
def set_local_events(tool_id: int, code: CodeType, event_set: int, /) -> int: ...
def restart_events() -> None: ...

DISABLE: Final[object]
MISSING: Final[object]

def register_callback(tool_id: int, event: int, func: Callable[..., Any] | None, /) -> Callable[..., Any] | None: ...
