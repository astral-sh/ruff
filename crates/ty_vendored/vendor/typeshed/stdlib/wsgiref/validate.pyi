"""
Middleware to check for obedience to the WSGI specification.

Some of the things this checks:

* Signature of the application and start_response (including that
  keyword arguments are not used).

* Environment checks:

  - Environment is a dictionary (and not a subclass).

  - That all the required keys are in the environment: REQUEST_METHOD,
    SERVER_NAME, SERVER_PORT, wsgi.version, wsgi.input, wsgi.errors,
    wsgi.multithread, wsgi.multiprocess, wsgi.run_once

  - That HTTP_CONTENT_TYPE and HTTP_CONTENT_LENGTH are not in the
    environment (these headers should appear as CONTENT_LENGTH and
    CONTENT_TYPE).

  - Warns if QUERY_STRING is missing, as the cgi module acts
    unpredictably in that case.

  - That CGI-style variables (that don't contain a .) have
    (non-unicode) string values

  - That wsgi.version is a tuple

  - That wsgi.url_scheme is 'http' or 'https' (@@: is this too
    restrictive?)

  - Warns if the REQUEST_METHOD is not known (@@: probably too
    restrictive).

  - That SCRIPT_NAME and PATH_INFO are empty or start with /

  - That at least one of SCRIPT_NAME or PATH_INFO are set.

  - That CONTENT_LENGTH is a positive integer.

  - That SCRIPT_NAME is not '/' (it should be '', and PATH_INFO should
    be '/').

  - That wsgi.input has the methods read, readline, readlines, and
    __iter__

  - That wsgi.errors has the methods flush, write, writelines

* The status is a string, contains a space, starts with an integer,
  and that integer is in range (> 100).

* That the headers is a list (not a subclass, not another kind of
  sequence).

* That the items of the headers are tuples of strings.

* That there is no 'status' header (that is used in CGI, but not in
  WSGI).

* That the headers don't contain newlines or colons, end in _ or -, or
  contain characters codes below 037.

* That Content-Type is given if there is content (CGI often has a
  default content type, but WSGI does not).

* That no Content-Type is given when there is no content (@@: is this
  too restrictive?)

* That the exc_info argument to start_response is a tuple or None.

* That all calls to the writer are with strings, and no other methods
  on the writer are accessed.

* That wsgi.input is used properly:

  - .read() is called with exactly one argument

  - That it returns a string

  - That readline, readlines, and __iter__ return strings

  - That .close() is not called

  - No other methods are provided

* That wsgi.errors is used properly:

  - .write() and .writelines() is called with a string

  - That .close() is not called, and no other methods are provided.

* The response iterator:

  - That it is not a string (it should be a list of a single string; a
    string will work, but perform horribly).

  - That .__next__() returns a string

  - That the iterator is not iterated over until start_response has
    been called (that can signal either a server or application
    error).

  - That .close() is called (doesn't raise exception, only prints to
    sys.stderr, because we only know it isn't called when the object
    is garbage collected).
"""

from _typeshed.wsgi import ErrorStream, InputStream, WSGIApplication
from collections.abc import Callable, Iterable, Iterator
from typing import Any, NoReturn
from typing_extensions import TypeAlias

__all__ = ["validator"]

class WSGIWarning(Warning):
    """
    Raised in response to WSGI-spec-related warnings
    """

def validator(application: WSGIApplication) -> WSGIApplication:
    """
    When applied between a WSGI server and a WSGI application, this
    middleware will check for WSGI compliance on a number of levels.
    This middleware does not modify the request or response in any
    way, but will raise an AssertionError if anything seems off
    (except for a failure to close the application iterator, which
    will be printed to stderr -- there's no way to raise an exception
    at that point).
    """

class InputWrapper:
    input: InputStream
    def __init__(self, wsgi_input: InputStream) -> None: ...
    def read(self, size: int) -> bytes: ...
    def readline(self, size: int = ...) -> bytes: ...
    def readlines(self, hint: int = ...) -> bytes: ...
    def __iter__(self) -> Iterator[bytes]: ...
    def close(self) -> NoReturn: ...

class ErrorWrapper:
    errors: ErrorStream
    def __init__(self, wsgi_errors: ErrorStream) -> None: ...
    def write(self, s: str) -> None: ...
    def flush(self) -> None: ...
    def writelines(self, seq: Iterable[str]) -> None: ...
    def close(self) -> NoReturn: ...

_WriterCallback: TypeAlias = Callable[[bytes], Any]

class WriteWrapper:
    writer: _WriterCallback
    def __init__(self, wsgi_writer: _WriterCallback) -> None: ...
    def __call__(self, s: bytes) -> None: ...

class PartialIteratorWrapper:
    iterator: Iterator[bytes]
    def __init__(self, wsgi_iterator: Iterator[bytes]) -> None: ...
    def __iter__(self) -> IteratorWrapper: ...

class IteratorWrapper:
    original_iterator: Iterator[bytes]
    iterator: Iterator[bytes]
    closed: bool
    check_start_response: bool | None
    def __init__(self, wsgi_iterator: Iterator[bytes], check_start_response: bool | None) -> None: ...
    def __iter__(self) -> IteratorWrapper: ...
    def __next__(self) -> bytes: ...
    def close(self) -> None: ...
    def __del__(self) -> None: ...
