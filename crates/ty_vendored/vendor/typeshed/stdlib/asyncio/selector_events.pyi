"""Event loop using a selector and related classes.

A selector is a "notify-when-ready" multiplexer.  For a subclass which
also includes support for signal handling, see the unix_events sub-module.
"""

import selectors
from socket import socket

from . import base_events

__all__ = ("BaseSelectorEventLoop",)

class BaseSelectorEventLoop(base_events.BaseEventLoop):
    """Selector event loop.

    See events.EventLoop for API specification.
    """

    def __init__(self, selector: selectors.BaseSelector | None = None) -> None: ...
    async def sock_recv(self, sock: socket, n: int) -> bytes:
        """Receive data from the socket.

        The return value is a bytes object representing the data received.
        The maximum amount of data to be received at once is specified by
        nbytes.
        """
