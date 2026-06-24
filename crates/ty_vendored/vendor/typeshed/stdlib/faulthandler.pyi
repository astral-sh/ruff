"""faulthandler module."""
import sys
from _typeshed import FileDescriptorLike

def cancel_dump_traceback_later() -> None:
    """Cancel the previous call to dump_traceback_later()."""
def disable() -> None:
    """Disable the fault handler."""

if sys.version_info >= (3, 15):
    def dump_traceback(
        file: FileDescriptorLike = sys.stderr, all_threads: bool = True, *, max_threads: int | None = None
    ) -> None:
        """Dump the traceback of the current thread into file.

Dump the traceback of all threads if all_threads is true. max_threads
caps the number of threads dumped.
"""

else:
    def dump_traceback(file: FileDescriptorLike = sys.stderr, all_threads: bool = True) -> None:
        """Dump the traceback of the current thread, or of all threads if all_threads is True, into file."""

if sys.version_info >= (3, 14):
    def dump_c_stack(file: FileDescriptorLike = sys.stderr) -> None:
        """Dump the C stack of the current thread."""

if sys.version_info >= (3, 15):
    def dump_traceback_later(
        timeout: float,
        repeat: bool = False,
        file: FileDescriptorLike = sys.stderr,
        exit: bool = False,
        *,
        max_threads: int | None = None,
    ) -> None:
        """Dump the traceback of all threads in timeout seconds.

If repeat is true, the tracebacks of all threads are dumped every
timeout seconds.  If exit is true, call _exit(1) which is not safe.
max_threads caps the number of threads dumped.
"""

else:
    def dump_traceback_later(
        timeout: float, repeat: bool = False, file: FileDescriptorLike = sys.stderr, exit: bool = False
    ) -> None:
        """Dump the traceback of all threads in timeout seconds,
or each timeout seconds if repeat is True. If exit is True, call _exit(1) which is not safe.
"""

if sys.version_info >= (3, 15):
    def enable(
        file: FileDescriptorLike = sys.stderr, all_threads: bool = True, c_stack: bool = True, *, max_threads: int | None = None
    ) -> None:
        """Enable the fault handler."""

elif sys.version_info >= (3, 14):
    def enable(file: FileDescriptorLike = sys.stderr, all_threads: bool = True, c_stack: bool = True) -> None:
        """Enable the fault handler."""

else:
    def enable(file: FileDescriptorLike = sys.stderr, all_threads: bool = True) -> None:
        """Enable the fault handler."""

def is_enabled() -> bool:
    """Check if the handler is enabled."""

if sys.platform != "win32":
    if sys.version_info >= (3, 15):
        def register(
            signum: int,
            file: FileDescriptorLike = sys.stderr,
            all_threads: bool = True,
            chain: bool = False,
            *,
            max_threads: int | None = None,
        ) -> None:
            """Register a handler for the signal 'signum'.

Dump the traceback of the current thread, or of all threads if
all_threads is True, into file. max_threads caps the number of threads
dumped.
"""
    else:
        def register(
            signum: int, file: FileDescriptorLike = sys.stderr, all_threads: bool = True, chain: bool = False
        ) -> None:
            """Register a handler for the signal 'signum': dump the traceback of the current thread, or of all threads if all_threads is True, into file."""

    def unregister(signum: int, /) -> None:
        """Unregister the handler of the signal 'signum' registered by register()."""
