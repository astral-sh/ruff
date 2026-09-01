"""Debug module to trace memory blocks allocated by Python."""

from collections.abc import Sequence
from tracemalloc import _FrameTuple, _TraceTuple

def _get_object_traceback(obj: object, /) -> Sequence[_FrameTuple] | None:
    """Get the traceback where the Python object obj was allocated.

    Return a tuple of (filename: str, lineno: int) tuples.
    Return None if the tracemalloc module is disabled or did not
    trace the allocation of the object.
    """

def _get_traces() -> Sequence[_TraceTuple]:
    """Get traces of all memory blocks allocated by Python.

    Return a list of (size: int, traceback: tuple) tuples.
    traceback is a tuple of (filename: str, lineno: int) tuples.

    Return an empty list if the tracemalloc module is disabled.
    """

def clear_traces() -> None:
    """Clear traces of memory blocks allocated by Python."""

def get_traceback_limit() -> int:
    """Get the maximum number of frames stored in the traceback of a trace.

    By default, a trace of an allocated memory block only stores
    the most recent frame: the limit is 1.
    """

def get_traced_memory() -> tuple[int, int]:
    """Get the current size and peak size of memory blocks traced by tracemalloc.

    Returns a tuple: (current: int, peak: int).
    """

def get_tracemalloc_memory() -> int:
    """Get the memory usage in bytes of the tracemalloc module.

    This memory is used internally to trace memory allocations.
    """

def is_tracing() -> bool:
    """Return True if the tracemalloc module is tracing Python memory allocations."""

def reset_peak() -> None:
    """Set the peak size of memory blocks traced by tracemalloc to the current size.

    Do nothing if the tracemalloc module is not tracing memory allocations.
    """

def start(nframe: int = 1, /) -> None:
    """Start tracing Python memory allocations.

    Also set the maximum number of frames stored in the traceback of a
    trace to nframe.
    """

def stop() -> None:
    """Stop tracing Python memory allocations.

    Also clear traces of memory blocks allocated by Python.
    """
