import sys
from socket import socket

__all__ = ["stop"]

if sys.platform == "win32":
    __all__ += ["DupSocket"]

    class DupSocket:
        """Picklable wrapper for a socket."""

        def __init__(self, sock: socket) -> None: ...
        def detach(self) -> socket:
            """Get the socket.  This should only be called once."""

else:
    __all__ += ["DupFd"]

    class DupFd:
        """Wrapper for fd which can be used at any time."""

        def __init__(self, fd: int) -> None: ...
        def detach(self) -> int:
            """Get the fd.  This should only be called once."""

def stop(timeout: float | None = None) -> None:
    """Stop the background thread and clear registered resources."""
