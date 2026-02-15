"""asyncio exceptions."""

import sys

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 11):
    __all__ = (
        "BrokenBarrierError",
        "CancelledError",
        "InvalidStateError",
        "TimeoutError",
        "IncompleteReadError",
        "LimitOverrunError",
        "SendfileNotAvailableError",
    )
else:
    __all__ = (
        "CancelledError",
        "InvalidStateError",
        "TimeoutError",
        "IncompleteReadError",
        "LimitOverrunError",
        "SendfileNotAvailableError",
    )

class CancelledError(BaseException):
    """The Future or Task was cancelled."""

if sys.version_info >= (3, 11):
    from builtins import TimeoutError as TimeoutError
else:
    class TimeoutError(Exception):
        """The operation exceeded the given deadline."""

class InvalidStateError(Exception):
    """The operation is not allowed in this state."""

class SendfileNotAvailableError(RuntimeError):
    """Sendfile syscall is not available.

    Raised if OS does not support sendfile syscall for given socket or
    file type.
    """

class IncompleteReadError(EOFError):
    """
    Incomplete read error. Attributes:

    - partial: read bytes string before the end of stream was reached
    - expected: total number of expected bytes (or None if unknown)
    """

    expected: int | None
    partial: bytes
    def __init__(self, partial: bytes, expected: int | None) -> None: ...

class LimitOverrunError(Exception):
    """Reached the buffer limit while looking for a separator.

    Attributes:
    - consumed: total number of to be consumed bytes.
    """

    consumed: int
    def __init__(self, message: str, consumed: int) -> None: ...

if sys.version_info >= (3, 11):
    class BrokenBarrierError(RuntimeError):
        """Barrier is broken by barrier.abort() call."""
