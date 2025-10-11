"""Loading unittests."""

import sys
import unittest.case
import unittest.suite
from collections.abc import Callable, Sequence
from re import Pattern
from types import ModuleType
from typing import Any, Final
from typing_extensions import TypeAlias, deprecated

_SortComparisonMethod: TypeAlias = Callable[[str, str], int]
_SuiteClass: TypeAlias = Callable[[list[unittest.case.TestCase]], unittest.suite.TestSuite]

VALID_MODULE_NAME: Final[Pattern[str]]

class TestLoader:
    """
    This class is responsible for loading tests according to various criteria
    and returning them wrapped in a TestSuite
    """

    errors: list[type[BaseException]]
    testMethodPrefix: str
    sortTestMethodsUsing: _SortComparisonMethod
    testNamePatterns: list[str] | None
    suiteClass: _SuiteClass
    def loadTestsFromTestCase(self, testCaseClass: type[unittest.case.TestCase]) -> unittest.suite.TestSuite:
        """Return a suite of all test cases contained in testCaseClass"""
    if sys.version_info >= (3, 12):
        def loadTestsFromModule(self, module: ModuleType, *, pattern: str | None = None) -> unittest.suite.TestSuite:
            """Return a suite of all test cases contained in the given module"""
    else:
        def loadTestsFromModule(self, module: ModuleType, *args: Any, pattern: str | None = None) -> unittest.suite.TestSuite:
            """Return a suite of all test cases contained in the given module"""

    def loadTestsFromName(self, name: str, module: ModuleType | None = None) -> unittest.suite.TestSuite:
        """Return a suite of all test cases given a string specifier.

        The name may resolve either to a module, a test case class, a
        test method within a test case class, or a callable object which
        returns a TestCase or TestSuite instance.

        The method optionally resolves the names relative to a given module.
        """

    def loadTestsFromNames(self, names: Sequence[str], module: ModuleType | None = None) -> unittest.suite.TestSuite:
        """Return a suite of all test cases found using the given sequence
        of string specifiers. See 'loadTestsFromName()'.
        """

    def getTestCaseNames(self, testCaseClass: type[unittest.case.TestCase]) -> Sequence[str]:
        """Return a sorted sequence of method names found within testCaseClass"""

    def discover(self, start_dir: str, pattern: str = "test*.py", top_level_dir: str | None = None) -> unittest.suite.TestSuite:
        """Find and return all test modules from the specified start
        directory, recursing into subdirectories to find them and return all
        tests found within them. Only test files that match the pattern will
        be loaded. (Using shell style pattern matching.)

        All test modules must be importable from the top level of the project.
        If the start directory is not the top level directory then the top
        level directory must be specified separately.

        If a test package name (directory with '__init__.py') matches the
        pattern then the package will be checked for a 'load_tests' function. If
        this exists then it will be called with (loader, tests, pattern) unless
        the package has already had load_tests called from the same discovery
        invocation, in which case the package module object is not scanned for
        tests - this ensures that when a package uses discover to further
        discover child tests that infinite recursion does not happen.

        If load_tests exists then discovery does *not* recurse into the package,
        load_tests is responsible for loading all tests in the package.

        The pattern is deliberately not stored as a loader attribute so that
        packages can continue discovery themselves. top_level_dir is stored so
        load_tests does not need to pass this argument in to loader.discover().

        Paths are sorted before being imported to ensure reproducible execution
        order even on filesystems with non-alphabetical ordering like ext3/4.
        """

    def _match_path(self, path: str, full_path: str, pattern: str) -> bool: ...

defaultTestLoader: TestLoader

if sys.version_info < (3, 13):
    if sys.version_info >= (3, 11):
        @deprecated("Deprecated since Python 3.11; removed in Python 3.13.")
        def getTestCaseNames(
            testCaseClass: type[unittest.case.TestCase],
            prefix: str,
            sortUsing: _SortComparisonMethod = ...,
            testNamePatterns: list[str] | None = None,
        ) -> Sequence[str]: ...
        @deprecated("Deprecated since Python 3.11; removed in Python 3.13.")
        def makeSuite(
            testCaseClass: type[unittest.case.TestCase],
            prefix: str = "test",
            sortUsing: _SortComparisonMethod = ...,
            suiteClass: _SuiteClass = ...,
        ) -> unittest.suite.TestSuite: ...
        @deprecated("Deprecated since Python 3.11; removed in Python 3.13.")
        def findTestCases(
            module: ModuleType, prefix: str = "test", sortUsing: _SortComparisonMethod = ..., suiteClass: _SuiteClass = ...
        ) -> unittest.suite.TestSuite: ...
    else:
        def getTestCaseNames(
            testCaseClass: type[unittest.case.TestCase],
            prefix: str,
            sortUsing: _SortComparisonMethod = ...,
            testNamePatterns: list[str] | None = None,
        ) -> Sequence[str]: ...
        def makeSuite(
            testCaseClass: type[unittest.case.TestCase],
            prefix: str = "test",
            sortUsing: _SortComparisonMethod = ...,
            suiteClass: _SuiteClass = ...,
        ) -> unittest.suite.TestSuite: ...
        def findTestCases(
            module: ModuleType, prefix: str = "test", sortUsing: _SortComparisonMethod = ..., suiteClass: _SuiteClass = ...
        ) -> unittest.suite.TestSuite: ...
