"""FeedParser - An email feed parser.

The feed parser implements an interface for incrementally parsing an email
message, line by line.  This has advantages for certain applications, such as
those reading email messages off a socket.

FeedParser.feed() is the primary interface for pushing new data into the
parser.  It returns when there's nothing more it can do with the available
data.  When you have no more data to push into the parser, call .close().
This completes the parsing and returns the root message object.

The other advantage of this parser is that it will never raise a parsing
exception.  Instead, when it finds something unexpected, it adds a 'defect' to
the current message.  Defects are just instances that live on the message
object's .defects attribute.
"""

from collections.abc import Callable
from email._policybase import _MessageT
from email.message import Message
from email.policy import Policy
from typing import Generic, overload

__all__ = ["FeedParser", "BytesFeedParser"]

class FeedParser(Generic[_MessageT]):
    """A feed-style parser of email."""

    @overload
    def __init__(self: FeedParser[Message], _factory: None = None, *, policy: Policy[Message] = ...) -> None:
        """_factory is called with no arguments to create a new message obj

        The policy keyword specifies a policy object that controls a number of
        aspects of the parser's operation.  The default policy maintains
        backward compatibility.

        """

    @overload
    def __init__(self, _factory: Callable[[], _MessageT], *, policy: Policy[_MessageT] = ...) -> None: ...
    def feed(self, data: str) -> None:
        """Push more data into the parser."""

    def close(self) -> _MessageT:
        """Parse all remaining data and return the root message object."""

class BytesFeedParser(FeedParser[_MessageT]):
    """Like FeedParser, but feed accepts bytes."""

    @overload
    def __init__(self: BytesFeedParser[Message], _factory: None = None, *, policy: Policy[Message] = ...) -> None:
        """_factory is called with no arguments to create a new message obj

        The policy keyword specifies a policy object that controls a number of
        aspects of the parser's operation.  The default policy maintains
        backward compatibility.

        """

    @overload
    def __init__(self, _factory: Callable[[], _MessageT], *, policy: Policy[_MessageT] = ...) -> None: ...
    def feed(self, data: bytes | bytearray) -> None: ...  # type: ignore[override]
