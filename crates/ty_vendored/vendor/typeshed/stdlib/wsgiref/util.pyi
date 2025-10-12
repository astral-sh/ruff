"""Miscellaneous WSGI-related Utilities"""

import sys
from _typeshed.wsgi import WSGIEnvironment
from collections.abc import Callable
from typing import IO, Any

__all__ = ["FileWrapper", "guess_scheme", "application_uri", "request_uri", "shift_path_info", "setup_testing_defaults"]
if sys.version_info >= (3, 13):
    __all__ += ["is_hop_by_hop"]

class FileWrapper:
    """Wrapper to convert file-like objects to iterables"""

    filelike: IO[bytes]
    blksize: int
    close: Callable[[], None]  # only exists if filelike.close exists
    def __init__(self, filelike: IO[bytes], blksize: int = 8192) -> None: ...
    if sys.version_info < (3, 11):
        def __getitem__(self, key: Any) -> bytes: ...

    def __iter__(self) -> FileWrapper: ...
    def __next__(self) -> bytes: ...

def guess_scheme(environ: WSGIEnvironment) -> str:
    """Return a guess for whether 'wsgi.url_scheme' should be 'http' or 'https'"""

def application_uri(environ: WSGIEnvironment) -> str:
    """Return the application's base URI (no PATH_INFO or QUERY_STRING)"""

def request_uri(environ: WSGIEnvironment, include_query: bool = True) -> str:
    """Return the full request URI, optionally including the query string"""

def shift_path_info(environ: WSGIEnvironment) -> str | None:
    """Shift a name from PATH_INFO to SCRIPT_NAME, returning it

    If there are no remaining path segments in PATH_INFO, return None.
    Note: 'environ' is modified in-place; use a copy if you need to keep
    the original PATH_INFO or SCRIPT_NAME.

    Note: when PATH_INFO is just a '/', this returns '' and appends a trailing
    '/' to SCRIPT_NAME, even though empty path segments are normally ignored,
    and SCRIPT_NAME doesn't normally end in a '/'.  This is intentional
    behavior, to ensure that an application can tell the difference between
    '/x' and '/x/' when traversing to objects.
    """

def setup_testing_defaults(environ: WSGIEnvironment) -> None:
    """Update 'environ' with trivial defaults for testing purposes

    This adds various parameters required for WSGI, including HTTP_HOST,
    SERVER_NAME, SERVER_PORT, REQUEST_METHOD, SCRIPT_NAME, PATH_INFO,
    and all of the wsgi.* variables.  It only supplies default values,
    and does not replace any existing settings for these variables.

    This routine is intended to make it easier for unit tests of WSGI
    servers and applications to set up dummy environments.  It should *not*
    be used by actual WSGI servers or applications, since the data is fake!
    """

def is_hop_by_hop(header_name: str) -> bool:
    """Return true if 'header_name' is an HTTP/1.1 "Hop-by-Hop" header"""
