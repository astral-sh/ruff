"""Exception classes raised by urllib.

The base exception class is URLError, which inherits from OSError.  It
doesn't define any behavior of its own, but is the base class for all
exceptions defined in this package.

HTTPError is an exception class that is also a valid HTTP response
instance.  It behaves this way because HTTP protocol errors are valid
responses, with a status code, headers, and a body.  In some contexts,
an application may want to handle an exception like a regular
response.
"""

from email.message import Message
from typing import IO
from urllib.response import addinfourl

__all__ = ["URLError", "HTTPError", "ContentTooShortError"]

class URLError(OSError):
    reason: str | BaseException
    # The `filename` attribute only exists if it was provided to `__init__` and wasn't `None`.
    filename: str
    def __init__(self, reason: str | BaseException, filename: str | None = None) -> None: ...

class HTTPError(URLError, addinfourl):
    """Raised when HTTP error occurs, but also acts like non-error return"""

    @property
    def headers(self) -> Message: ...
    @headers.setter
    def headers(self, headers: Message) -> None: ...
    @property
    def reason(self) -> str: ...  # type: ignore[override]
    code: int
    msg: str
    hdrs: Message
    fp: IO[bytes]
    def __init__(self, url: str, code: int, msg: str, hdrs: Message, fp: IO[bytes] | None) -> None: ...

class ContentTooShortError(URLError):
    """Exception raised when downloaded size does not match content-length."""

    content: tuple[str, Message]
    def __init__(self, message: str, content: tuple[str, Message]) -> None: ...
