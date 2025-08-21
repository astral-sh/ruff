"""
Python unit testing framework, based on Erich Gamma's JUnit and Kent Beck's
Smalltalk testing framework (used with permission).

This module contains the core framework classes that form the basis of
specific test cases and suites (TestCase, TestSuite etc.), and also a
text-based utility class for running the tests and reporting the results
 (TextTestRunner).

Simple usage:

    import unittest

    class IntegerArithmeticTestCase(unittest.TestCase):
        def testAdd(self):  # test method names begin with 'test'
            self.assertEqual((1 + 2), 3)
            self.assertEqual(0 + 1, 1)
        def testMultiply(self):
            self.assertEqual((0 * 10), 0)
            self.assertEqual((5 * 8), 40)

    if __name__ == '__main__':
        unittest.main()

Further information is available in the bundled documentation, and from

  http://docs.python.org/library/unittest.html

Copyright (c) 1999-2003 Steve Purcell
Copyright (c) 2003 Python Software Foundation
This module is free software, and you may redistribute it and/or modify
it under the same terms as Python itself, so long as this copyright message
and disclaimer are retained in their original form.

IN NO EVENT SHALL THE AUTHOR BE LIABLE TO ANY PARTY FOR DIRECT, INDIRECT,
SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES ARISING OUT OF THE USE OF
THIS CODE, EVEN IF THE AUTHOR HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH
DAMAGE.

THE AUTHOR SPECIFICALLY DISCLAIMS ANY WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE.  THE CODE PROVIDED HEREUNDER IS ON AN "AS IS" BASIS,
AND THERE IS NO OBLIGATION WHATSOEVER TO PROVIDE MAINTENANCE,
SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
"""

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
