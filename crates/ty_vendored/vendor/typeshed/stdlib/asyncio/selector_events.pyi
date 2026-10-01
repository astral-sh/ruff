import selectors
from socket import socket

from . import base_events

__all__ = ("BaseSelectorEventLoop",)

class BaseSelectorEventLoop(base_events.BaseEventLoop):
    def __init__(self, selector: selectors.BaseSelector | None = None) -> None: ...
    async def sock_recv(self, sock: socket, n: int) -> bytes: ...
