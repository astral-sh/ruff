"""Running tests"""

import sys
import unittest.case
import unittest.result
import unittest.suite
from _typeshed import SupportsFlush, SupportsWrite
from collections.abc import Callable, Iterable
from typing import Any, Generic, Protocol, TypeVar, type_check_only
from typing_extensions import Never, TypeAlias
from warnings import _ActionKind

_ResultClassType: TypeAlias = Callable[[_TextTestStream, bool, int], TextTestResult[Any]]

@type_check_only
class _SupportsWriteAndFlush(SupportsWrite[str], SupportsFlush, Protocol): ...

# All methods used by unittest.runner.TextTestResult's stream
@type_check_only
class _TextTestStream(_SupportsWriteAndFlush, Protocol):
    def writeln(self, arg: str | None = None, /) -> None: ...

# _WritelnDecorator should have all the same attrs as its stream param.
# But that's not feasible to do Generically
# We can expand the attributes if requested
class _WritelnDecorator:
    """Used to decorate file-like objects with a handy 'writeln' method"""

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
    """A test result class that can print formatted text results to a stream.

    Used by TextTestRunner.
    """

    descriptions: bool  # undocumented
    dots: bool  # undocumented
    separator1: str
    separator2: str
    showAll: bool  # undocumented
    stream: _StreamT  # undocumented
    if sys.version_info >= (3, 12):
        durations: int | None
        def __init__(self, stream: _StreamT, descriptions: bool, verbosity: int, *, durations: int | None = None) -> None:
            """Construct a TextTestResult. Subclasses should accept **kwargs
            to ensure compatibility as the interface changes.
            """
    else:
        def __init__(self, stream: _StreamT, descriptions: bool, verbosity: int) -> None: ...

    def getDescription(self, test: unittest.case.TestCase) -> str: ...
    def printErrorList(self, flavour: str, errors: Iterable[tuple[unittest.case.TestCase, str]]) -> None: ...

class TextTestRunner:
    """A test runner class that displays results in textual form.

    It prints out the names of tests as they are run, errors as they
    occur, and a summary of the results at the end of the test run.
    """

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
        ) -> None:
            """Construct a TextTestRunner.

            Subclasses should accept **kwargs to ensure compatibility as the
            interface changes.
            """
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
        ) -> None:
            """Construct a TextTestRunner.

            Subclasses should accept **kwargs to ensure compatibility as the
            interface changes.
            """

    def _makeResult(self) -> TextTestResult: ...
    def run(self, test: unittest.suite.TestSuite | unittest.case.TestCase) -> TextTestResult:
        """Run the given test case or test suite."""
