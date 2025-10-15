"""faulthandler module."""

import sys
from _typeshed import FileDescriptorLike

def cancel_dump_traceback_later() -> None:
    """Cancel the previous call to dump_traceback_later()."""

def disable() -> None:
    """Disable the fault handler."""

def dump_traceback(file: FileDescriptorLike = ..., all_threads: bool = ...) -> None:
    """Dump the traceback of the current thread, or of all threads if all_threads is True, into file."""

if sys.version_info >= (3, 14):
    def dump_c_stack(file: FileDescriptorLike = ...) -> None:
        """Dump the C stack of the current thread."""

def dump_traceback_later(timeout: float, repeat: bool = ..., file: FileDescriptorLike = ..., exit: bool = ...) -> None:
    """Dump the traceback of all threads in timeout seconds,
    or each timeout seconds if repeat is True. If exit is True, call _exit(1) which is not safe.
    """

if sys.version_info >= (3, 14):
    def enable(file: FileDescriptorLike = ..., all_threads: bool = ..., c_stack: bool = True) -> None:
        """Enable the fault handler."""

else:
    def enable(file: FileDescriptorLike = ..., all_threads: bool = ...) -> None:
        """Enable the fault handler."""

def is_enabled() -> bool:
    """Check if the handler is enabled."""

if sys.platform != "win32":
    def register(signum: int, file: FileDescriptorLike = ..., all_threads: bool = ..., chain: bool = ...) -> None:
        """Register a handler for the signal 'signum': dump the traceback of the current thread, or of all threads if all_threads is True, into file."""

    def unregister(signum: int, /) -> None:
        """Unregister the handler of the signal 'signum' registered by register()."""
