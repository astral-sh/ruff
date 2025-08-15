"""
The objects used by the site module to add custom builtins.
"""

import sys
from collections.abc import Iterable
from typing import ClassVar, Literal, NoReturn

class Quitter:
    name: str
    eof: str
    def __init__(self, name: str, eof: str) -> None: ...
    def __call__(self, code: sys._ExitCode = None) -> NoReturn: ...

class _Printer:
    """interactive prompt objects for printing the license text, a list of
    contributors and the copyright notice.
    """

    MAXLINES: ClassVar[Literal[23]]
    def __init__(self, name: str, data: str, files: Iterable[str] = (), dirs: Iterable[str] = ()) -> None: ...
    def __call__(self) -> None: ...

class _Helper:
    """Define the builtin 'help'.

    This is a wrapper around pydoc.help that provides a helpful message
    when 'help' is typed at the Python interactive prompt.

    Calling help() at the Python prompt starts an interactive help session.
    Calling help(thing) prints help for the python object 'thing'.
    """

    def __call__(self, request: object = ...) -> None: ...
