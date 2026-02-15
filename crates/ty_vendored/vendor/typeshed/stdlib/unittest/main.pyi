import sys
import unittest.case
import unittest.loader
import unittest.result
import unittest.suite
from collections.abc import Iterable
from types import ModuleType
from typing import Any, Final, Protocol, type_check_only
from typing_extensions import deprecated

MAIN_EXAMPLES: Final[str]
MODULE_EXAMPLES: Final[str]

@type_check_only
class _TestRunner(Protocol):
    def run(self, test: unittest.suite.TestSuite | unittest.case.TestCase, /) -> unittest.result.TestResult: ...

# not really documented
class TestProgram:
    result: unittest.result.TestResult
    module: None | str | ModuleType
    verbosity: int
    failfast: bool | None
    catchbreak: bool | None
    buffer: bool | None
    progName: str | None
    warnings: str | None
    testNamePatterns: list[str] | None
    if sys.version_info >= (3, 12):
        durations: unittest.result._DurationsType | None
        def __init__(
            self,
            module: None | str | ModuleType = "__main__",
            defaultTest: str | Iterable[str] | None = None,
            argv: list[str] | None = None,
            testRunner: type[_TestRunner] | _TestRunner | None = None,
            testLoader: unittest.loader.TestLoader = ...,
            exit: bool = True,
            verbosity: int = 1,
            failfast: bool | None = None,
            catchbreak: bool | None = None,
            buffer: bool | None = None,
            warnings: str | None = None,
            *,
            tb_locals: bool = False,
            durations: unittest.result._DurationsType | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            module: None | str | ModuleType = "__main__",
            defaultTest: str | Iterable[str] | None = None,
            argv: list[str] | None = None,
            testRunner: type[_TestRunner] | _TestRunner | None = None,
            testLoader: unittest.loader.TestLoader = ...,
            exit: bool = True,
            verbosity: int = 1,
            failfast: bool | None = None,
            catchbreak: bool | None = None,
            buffer: bool | None = None,
            warnings: str | None = None,
            *,
            tb_locals: bool = False,
        ) -> None: ...

    if sys.version_info < (3, 13):
        if sys.version_info >= (3, 11):
            @deprecated("Deprecated since Python 3.11; removed in Python 3.13.")
            def usageExit(self, msg: Any = None) -> None: ...
        else:
            def usageExit(self, msg: Any = None) -> None: ...

    def parseArgs(self, argv: list[str]) -> None: ...
    def createTests(self, from_discovery: bool = False, Loader: unittest.loader.TestLoader | None = None) -> None: ...
    def runTests(self) -> None: ...  # undocumented

main = TestProgram
