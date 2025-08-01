"""A lexical analyzer class for simple shell-like syntaxes."""

import sys
from collections import deque
from collections.abc import Iterable
from io import TextIOWrapper
from typing import Literal, Protocol, overload, type_check_only
from typing_extensions import Self, deprecated

__all__ = ["shlex", "split", "quote", "join"]

@type_check_only
class _ShlexInstream(Protocol):
    def read(self, size: Literal[1], /) -> str: ...
    def readline(self) -> object: ...
    def close(self) -> object: ...

if sys.version_info >= (3, 12):
    def split(s: str | _ShlexInstream, comments: bool = False, posix: bool = True) -> list[str]:
        """Split the string *s* using shell-like syntax."""

else:
    @overload
    def split(s: str | _ShlexInstream, comments: bool = False, posix: bool = True) -> list[str]:
        """Split the string *s* using shell-like syntax."""

    @overload
    @deprecated("Passing None for 's' to shlex.split() is deprecated and will raise an error in Python 3.12.")
    def split(s: None, comments: bool = False, posix: bool = True) -> list[str]: ...

def join(split_command: Iterable[str]) -> str:
    """Return a shell-escaped string from *split_command*."""

def quote(s: str) -> str:
    """Return a shell-escaped version of the string *s*."""

# TODO: Make generic over infile once PEP 696 is implemented.
class shlex:
    """A lexical analyzer class for simple shell-like syntaxes."""

    commenters: str
    wordchars: str
    whitespace: str
    escape: str
    quotes: str
    escapedquotes: str
    whitespace_split: bool
    infile: str | None
    instream: _ShlexInstream
    source: str
    debug: int
    lineno: int
    token: str
    filestack: deque[tuple[str | None, _ShlexInstream, int]]
    eof: str | None
    @property
    def punctuation_chars(self) -> str: ...
    def __init__(
        self,
        instream: str | _ShlexInstream | None = None,
        infile: str | None = None,
        posix: bool = False,
        punctuation_chars: bool | str = False,
    ) -> None: ...
    def get_token(self) -> str | None:
        """Get a token from the input stream (or from stack if it's nonempty)"""

    def push_token(self, tok: str) -> None:
        """Push a token onto the stack popped by the get_token method"""

    def read_token(self) -> str | None: ...
    def sourcehook(self, newfile: str) -> tuple[str, TextIOWrapper] | None:
        """Hook called on a filename to be sourced."""

    def push_source(self, newstream: str | _ShlexInstream, newfile: str | None = None) -> None:
        """Push an input source onto the lexer's input source stack."""

    def pop_source(self) -> None:
        """Pop the input source stack."""

    def error_leader(self, infile: str | None = None, lineno: int | None = None) -> str:
        """Emit a C-compiler-like, Emacs-friendly error-message leader."""

    def __iter__(self) -> Self: ...
    def __next__(self) -> str: ...
