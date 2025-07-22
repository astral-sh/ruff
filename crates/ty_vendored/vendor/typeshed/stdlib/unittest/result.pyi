"""Test result object"""

import sys
import unittest.case
from _typeshed import OptExcInfo
from collections.abc import Callable
from typing import Any, Final, TextIO, TypeVar
from typing_extensions import TypeAlias

_F = TypeVar("_F", bound=Callable[..., Any])
_DurationsType: TypeAlias = list[tuple[str, float]]

STDOUT_LINE: Final[str]
STDERR_LINE: Final[str]

# undocumented
def failfast(method: _F) -> _F: ...

class TestResult:
    """Holder for test result information.

    Test results are automatically managed by the TestCase and TestSuite
    classes, and do not need to be explicitly manipulated by writers of tests.

    Each instance holds the total number of tests run, and collections of
    failures and errors that occurred among those test runs. The collections
    contain tuples of (testcase, exceptioninfo), where exceptioninfo is the
    formatted traceback of the error that occurred.
    """

    errors: list[tuple[unittest.case.TestCase, str]]
    failures: list[tuple[unittest.case.TestCase, str]]
    skipped: list[tuple[unittest.case.TestCase, str]]
    expectedFailures: list[tuple[unittest.case.TestCase, str]]
    unexpectedSuccesses: list[unittest.case.TestCase]
    shouldStop: bool
    testsRun: int
    buffer: bool
    failfast: bool
    tb_locals: bool
    if sys.version_info >= (3, 12):
        collectedDurations: _DurationsType

    def __init__(self, stream: TextIO | None = None, descriptions: bool | None = None, verbosity: int | None = None) -> None: ...
    def printErrors(self) -> None:
        """Called by TestRunner after test run"""

    def wasSuccessful(self) -> bool:
        """Tells whether or not this result was a success."""

    def stop(self) -> None:
        """Indicates that the tests should be aborted."""

    def startTest(self, test: unittest.case.TestCase) -> None:
        """Called when the given test is about to be run"""

    def stopTest(self, test: unittest.case.TestCase) -> None:
        """Called when the given test has been run"""

    def startTestRun(self) -> None:
        """Called once before any tests are executed.

        See startTest for a method called before each test.
        """

    def stopTestRun(self) -> None:
        """Called once after all tests are executed.

        See stopTest for a method called after each test.
        """

    def addError(self, test: unittest.case.TestCase, err: OptExcInfo) -> None:
        """Called when an error has occurred. 'err' is a tuple of values as
        returned by sys.exc_info().
        """

    def addFailure(self, test: unittest.case.TestCase, err: OptExcInfo) -> None:
        """Called when an error has occurred. 'err' is a tuple of values as
        returned by sys.exc_info().
        """

    def addSuccess(self, test: unittest.case.TestCase) -> None:
        """Called when a test has completed successfully"""

    def addSkip(self, test: unittest.case.TestCase, reason: str) -> None:
        """Called when a test is skipped."""

    def addExpectedFailure(self, test: unittest.case.TestCase, err: OptExcInfo) -> None:
        """Called when an expected failure/error occurred."""

    def addUnexpectedSuccess(self, test: unittest.case.TestCase) -> None:
        """Called when a test was expected to fail, but succeed."""

    def addSubTest(self, test: unittest.case.TestCase, subtest: unittest.case.TestCase, err: OptExcInfo | None) -> None:
        """Called at the end of a subtest.
        'err' is None if the subtest ended successfully, otherwise it's a
        tuple of values as returned by sys.exc_info().
        """
    if sys.version_info >= (3, 12):
        def addDuration(self, test: unittest.case.TestCase, elapsed: float) -> None:
            """Called when a test finished to run, regardless of its outcome.
            *test* is the test case corresponding to the test method.
            *elapsed* is the time represented in seconds, and it includes the
            execution of cleanup functions.
            """
