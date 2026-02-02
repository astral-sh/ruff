"""_warnings provides basic warning filtering support.
It is a helper module to speed up interpreter start-up.
"""

import sys
from typing import Any, overload

_defaultaction: str
_onceregistry: dict[Any, Any]
filters: list[tuple[str, str | None, type[Warning], str | None, int]]

if sys.version_info >= (3, 12):
    @overload
    def warn(
        message: str,
        category: type[Warning] | None = None,
        stacklevel: int = 1,
        source: Any | None = None,
        *,
        skip_file_prefixes: tuple[str, ...] = (),
    ) -> None:
        """Issue a warning, or maybe ignore it or raise an exception.

        message
          Text of the warning message.
        category
          The Warning category subclass. Defaults to UserWarning.
        stacklevel
          How far up the call stack to make this warning appear. A value of 2 for
          example attributes the warning to the caller of the code calling warn().
        source
          If supplied, the destroyed object which emitted a ResourceWarning
        skip_file_prefixes
          An optional tuple of module filename prefixes indicating frames to skip
          during stacklevel computations for stack frame attribution.
        """

    @overload
    def warn(
        message: Warning,
        category: Any = None,
        stacklevel: int = 1,
        source: Any | None = None,
        *,
        skip_file_prefixes: tuple[str, ...] = (),
    ) -> None: ...

else:
    @overload
    def warn(message: str, category: type[Warning] | None = None, stacklevel: int = 1, source: Any | None = None) -> None:
        """Issue a warning, or maybe ignore it or raise an exception."""

    @overload
    def warn(message: Warning, category: Any = None, stacklevel: int = 1, source: Any | None = None) -> None: ...

@overload
def warn_explicit(
    message: str,
    category: type[Warning],
    filename: str,
    lineno: int,
    module: str | None = ...,
    registry: dict[str | tuple[str, type[Warning], int], int] | None = None,
    module_globals: dict[str, Any] | None = None,
    source: Any | None = None,
) -> None:
    """Issue a warning, or maybe ignore it or raise an exception."""

@overload
def warn_explicit(
    message: Warning,
    category: Any,
    filename: str,
    lineno: int,
    module: str | None = None,
    registry: dict[str | tuple[str, type[Warning], int], int] | None = None,
    module_globals: dict[str, Any] | None = None,
    source: Any | None = None,
) -> None: ...
