import sys
from unittest.async_case import *

from .case import (
    FunctionTestCase as FunctionTestCase,
    SkipTest as SkipTest,
    TestCase as TestCase,
    addModuleCleanup as addModuleCleanup,
    expectedFailure as expectedFailure,
    skip as skip,
    skipIf as skipIf,
    skipUnless as skipUnless,
)
from .loader import TestLoader as TestLoader, defaultTestLoader as defaultTestLoader
from .main import TestProgram as TestProgram, main as main
from .result import TestResult as TestResult
from .runner import TextTestResult as TextTestResult, TextTestRunner as TextTestRunner
from .signals import (
    installHandler as installHandler,
    registerResult as registerResult,
    removeHandler as removeHandler,
    removeResult as removeResult,
)
from .suite import BaseTestSuite as BaseTestSuite, TestSuite as TestSuite

if sys.version_info >= (3, 11):
    from .case import doModuleCleanups as doModuleCleanups, enterModuleContext as enterModuleContext

__all__ = [
    "IsolatedAsyncioTestCase",
    "TestResult",
    "TestCase",
    "TestSuite",
    "TextTestRunner",
    "TestLoader",
    "FunctionTestCase",
    "main",
    "defaultTestLoader",
    "SkipTest",
    "skip",
    "skipIf",
    "skipUnless",
    "expectedFailure",
    "TextTestResult",
    "installHandler",
    "registerResult",
    "removeResult",
    "removeHandler",
    "addModuleCleanup",
]

if sys.version_info < (3, 13):
    from .loader import findTestCases as findTestCases, getTestCaseNames as getTestCaseNames, makeSuite as makeSuite

    __all__ += ["getTestCaseNames", "makeSuite", "findTestCases"]

if sys.version_info >= (3, 11):
    __all__ += ["enterModuleContext", "doModuleCleanups"]

if sys.version_info < (3, 12):
    def load_tests(loader: TestLoader, tests: TestSuite, pattern: str | None) -> TestSuite: ...

def __dir__() -> set[str]: ...
