import sys
import unittest.case
import unittest.result
import unittest.suite
from _typeshed import SupportsFlush, SupportsWrite
from collections.abc import Callable, Iterable
from typing import Any, Generic, Protocol, TypeVar
from typing_extensions import Never, TypeAlias
from warnings import _ActionKind

_ResultClassType: TypeAlias = Callable[[_TextTestStream, bool, int], TextTestResult[Any]]

class _SupportsWriteAndFlush(SupportsWrite[str], SupportsFlush, Protocol): ...

# All methods used by unittest.runner.TextTestResult's stream
class _TextTestStream(_SupportsWriteAndFlush, Protocol):
    def writeln(self, arg: str | None = None, /) -> None: ...

# _WritelnDecorator should have all the same attrs as its stream param.
# But that's not feasible to do Generically
# We can expand the attributes if requested
class _WritelnDecorator:
    def __init__(self, stream: _SupportsWriteAndFlush) -> None: ...
    def writeln(self, arg: str | None = None) -> None: ...
    def __getattr__(self, attr: str) -> Any: ...  # Any attribute from the stream type passed to __init__
    # These attributes are prevented by __getattr__
    stream: Never
    __getstate__: Never
    # Methods proxied from the wrapped stream object via __getattr__
    def flush(self) -> object: ...
    def write(self, s: str, /) -> object: ...

_StreamT = TypeVar("_StreamT", bound=_TextTestStream, default=_WritelnDecorator)

class TextTestResult(unittest.result.TestResult, Generic[_StreamT]):
    descriptions: bool  # undocumented
    dots: bool  # undocumented
    separator1: str
    separator2: str
    showAll: bool  # undocumented
    stream: _StreamT  # undocumented
    if sys.version_info >= (3, 12):
        durations: int | None
        def __init__(self, stream: _StreamT, descriptions: bool, verbosity: int, *, durations: int | None = None) -> None: ...
    else:
        def __init__(self, stream: _StreamT, descriptions: bool, verbosity: int) -> None: ...

    def getDescription(self, test: unittest.case.TestCase) -> str: ...
    def printErrorList(self, flavour: str, errors: Iterable[tuple[unittest.case.TestCase, str]]) -> None: ...

class TextTestRunner:
    resultclass: _ResultClassType
    stream: _WritelnDecorator
    descriptions: bool
    verbosity: int
    failfast: bool
    buffer: bool
    warnings: _ActionKind | None
    tb_locals: bool

    if sys.version_info >= (3, 12):
        durations: int | None
        def __init__(
            self,
            stream: _SupportsWriteAndFlush | None = None,
            descriptions: bool = True,
            verbosity: int = 1,
            failfast: bool = False,
            buffer: bool = False,
            resultclass: _ResultClassType | None = None,
            warnings: _ActionKind | None = None,
            *,
            tb_locals: bool = False,
            durations: int | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            stream: _SupportsWriteAndFlush | None = None,
            descriptions: bool = True,
            verbosity: int = 1,
            failfast: bool = False,
            buffer: bool = False,
            resultclass: _ResultClassType | None = None,
            warnings: str | None = None,
            *,
            tb_locals: bool = False,
        ) -> None: ...

    def _makeResult(self) -> TextTestResult: ...
    def run(self, test: unittest.suite.TestSuite | unittest.case.TestCase) -> TextTestResult: ...
