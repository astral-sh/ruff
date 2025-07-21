from collections.abc import Callable
from email._policybase import _MessageT
from email.message import Message
from email.policy import Policy
from typing import Generic, overload

__all__ = ["FeedParser", "BytesFeedParser"]

class FeedParser(Generic[_MessageT]):
    @overload
    def __init__(self: FeedParser[Message], _factory: None = None, *, policy: Policy[Message] = ...) -> None: ...
    @overload
    def __init__(self, _factory: Callable[[], _MessageT], *, policy: Policy[_MessageT] = ...) -> None: ...
    def feed(self, data: str) -> None: ...
    def close(self) -> _MessageT: ...

class BytesFeedParser(FeedParser[_MessageT]):
    @overload
    def __init__(self: BytesFeedParser[Message], _factory: None = None, *, policy: Policy[Message] = ...) -> None: ...
    @overload
    def __init__(self, _factory: Callable[[], _MessageT], *, policy: Policy[_MessageT] = ...) -> None: ...
    def feed(self, data: bytes | bytearray) -> None: ...  # type: ignore[override]
