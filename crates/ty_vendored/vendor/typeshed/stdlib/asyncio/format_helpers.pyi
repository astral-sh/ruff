import functools
import sys
import traceback
from collections.abc import Iterable
from types import FrameType, FunctionType
from typing import Any, overload
from typing_extensions import TypeAlias

class _HasWrapper:
    __wrapper__: _HasWrapper | FunctionType

_FuncType: TypeAlias = FunctionType | _HasWrapper | functools.partial[Any] | functools.partialmethod[Any]

@overload
def _get_function_source(func: _FuncType) -> tuple[str, int]: ...
@overload
def _get_function_source(func: object) -> tuple[str, int] | None: ...

if sys.version_info >= (3, 13):
    def _format_callback_source(func: object, args: Iterable[Any], *, debug: bool = False) -> str: ...
    def _format_args_and_kwargs(args: Iterable[Any], kwargs: dict[str, Any], *, debug: bool = False) -> str:
        """Format function arguments and keyword arguments.

        Special case for a single parameter: ('hello',) is formatted as ('hello').

        Note that this function only returns argument details when
        debug=True is specified, as arguments may contain sensitive
        information.
        """

    def _format_callback(
        func: object, args: Iterable[Any], kwargs: dict[str, Any], *, debug: bool = False, suffix: str = ""
    ) -> str: ...

else:
    def _format_callback_source(func: object, args: Iterable[Any]) -> str: ...
    def _format_args_and_kwargs(args: Iterable[Any], kwargs: dict[str, Any]) -> str:
        """Format function arguments and keyword arguments.

        Special case for a single parameter: ('hello',) is formatted as ('hello').
        """

    def _format_callback(func: object, args: Iterable[Any], kwargs: dict[str, Any], suffix: str = "") -> str: ...

def extract_stack(f: FrameType | None = None, limit: int | None = None) -> traceback.StackSummary:
    """Replacement for traceback.extract_stack() that only does the
    necessary work for asyncio debug mode.
    """
