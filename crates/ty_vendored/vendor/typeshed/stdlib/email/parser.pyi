"""A parser of RFC 5322 and MIME email messages."""

from _typeshed import SupportsRead
from collections.abc import Callable
from email._policybase import _MessageT
from email.feedparser import BytesFeedParser as BytesFeedParser, FeedParser as FeedParser
from email.message import Message
from email.policy import Policy
from io import _WrappedBuffer
from typing import Generic, overload

__all__ = ["Parser", "HeaderParser", "BytesParser", "BytesHeaderParser", "FeedParser", "BytesFeedParser"]

class Parser(Generic[_MessageT]):
    @overload
    def __init__(self: Parser[Message[str, str]], _class: None = None) -> None:
        """Parser of RFC 5322 and MIME email messages.

        Creates an in-memory object tree representing the email message, which
        can then be manipulated and turned over to a Generator to return the
        textual representation of the message.

        The string must be formatted as a block of RFC 5322 headers and header
        continuation lines, optionally preceded by a 'Unix-from' header.  The
        header block is terminated either by the end of the string or by a
        blank line.

        _class is the class to instantiate for new message objects when they
        must be created.  This class must have a constructor that can take
        zero arguments.  Default is Message.Message.

        The policy keyword specifies a policy object that controls a number of
        aspects of the parser's operation.  The default policy maintains
        backward compatibility.

        """

    @overload
    def __init__(self, _class: None = None, *, policy: Policy[_MessageT]) -> None: ...
    @overload
    def __init__(self, _class: Callable[[], _MessageT] | None, *, policy: Policy[_MessageT] = ...) -> None: ...
    def parse(self, fp: SupportsRead[str], headersonly: bool = False) -> _MessageT:
        """Create a message structure from the data in a file.

        Reads all the data from the file and returns the root of the message
        structure.  Optional headersonly is a flag specifying whether to stop
        parsing after reading the headers or not.  The default is False,
        meaning it parses the entire contents of the file.
        """

    def parsestr(self, text: str, headersonly: bool = False) -> _MessageT:
        """Create a message structure from a string.

        Returns the root of the message structure.  Optional headersonly is a
        flag specifying whether to stop parsing after reading the headers or
        not.  The default is False, meaning it parses the entire contents of
        the file.
        """

class HeaderParser(Parser[_MessageT]):
    def parse(self, fp: SupportsRead[str], headersonly: bool = True) -> _MessageT: ...
    def parsestr(self, text: str, headersonly: bool = True) -> _MessageT: ...

class BytesParser(Generic[_MessageT]):
    parser: Parser[_MessageT]
    @overload
    def __init__(self: BytesParser[Message[str, str]], _class: None = None) -> None:
        """Parser of binary RFC 5322 and MIME email messages.

        Creates an in-memory object tree representing the email message, which
        can then be manipulated and turned over to a Generator to return the
        textual representation of the message.

        The input must be formatted as a block of RFC 5322 headers and header
        continuation lines, optionally preceded by a 'Unix-from' header.  The
        header block is terminated either by the end of the input or by a
        blank line.

        _class is the class to instantiate for new message objects when they
        must be created.  This class must have a constructor that can take
        zero arguments.  Default is Message.Message.
        """

    @overload
    def __init__(self, _class: None = None, *, policy: Policy[_MessageT]) -> None: ...
    @overload
    def __init__(self, _class: Callable[[], _MessageT], *, policy: Policy[_MessageT] = ...) -> None: ...
    def parse(self, fp: _WrappedBuffer, headersonly: bool = False) -> _MessageT:
        """Create a message structure from the data in a binary file.

        Reads all the data from the file and returns the root of the message
        structure.  Optional headersonly is a flag specifying whether to stop
        parsing after reading the headers or not.  The default is False,
        meaning it parses the entire contents of the file.
        """

    def parsebytes(self, text: bytes | bytearray, headersonly: bool = False) -> _MessageT:
        """Create a message structure from a byte string.

        Returns the root of the message structure.  Optional headersonly is a
        flag specifying whether to stop parsing after reading the headers or
        not.  The default is False, meaning it parses the entire contents of
        the file.
        """

class BytesHeaderParser(BytesParser[_MessageT]):
    def parse(self, fp: _WrappedBuffer, headersonly: bool = True) -> _MessageT: ...
    def parsebytes(self, text: bytes | bytearray, headersonly: bool = True) -> _MessageT: ...
