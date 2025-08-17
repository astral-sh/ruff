"""Test case implementation"""

import logging
import sys
import unittest.result
from _typeshed import SupportsDunderGE, SupportsDunderGT, SupportsDunderLE, SupportsDunderLT, SupportsRSub, SupportsSub
from builtins import _ClassInfo
from collections.abc import Callable, Container, Iterable, Mapping, Sequence, Set as AbstractSet
from contextlib import AbstractContextManager
from re import Pattern
from types import GenericAlias, TracebackType
from typing import Any, AnyStr, Final, Generic, NoReturn, Protocol, SupportsAbs, SupportsRound, TypeVar, overload, type_check_only
from typing_extensions import Never, ParamSpec, Self
from unittest._log import _AssertLogsContext, _LoggingWatcher
from warnings import WarningMessage

_T = TypeVar("_T")
_S = TypeVar("_S", bound=SupportsSub[Any, Any])
_E = TypeVar("_E", bound=BaseException)
_FT = TypeVar("_FT", bound=Callable[..., Any])
_SB = TypeVar("_SB", str, bytes, bytearray)
_P = ParamSpec("_P")

DIFF_OMITTED: Final[str]

class _BaseTestCaseContext:
    test_case: TestCase
    def __init__(self, test_case: TestCase) -> None: ...

class _AssertRaisesBaseContext(_BaseTestCaseContext):
    expected: type[BaseException] | tuple[type[BaseException], ...]
    expected_regex: Pattern[str] | None
    obj_name: str | None
    msg: str | None

    def __init__(
        self,
        expected: type[BaseException] | tuple[type[BaseException], ...],
        test_case: TestCase,
        expected_regex: str | Pattern[str] | None = None,
    ) -> None: ...

    # This returns Self if args is the empty list, and None otherwise.
    # but it's not possible to construct an overload which expresses that
    def handle(self, name: str, args: list[Any], kwargs: dict[str, Any]) -> Any:
        """
        If args is empty, assertRaises/Warns is being used as a
        context manager, so check for a 'msg' kwarg and return self.
        If args is not empty, call a callable passing positional and keyword
        arguments.
        """

def addModuleCleanup(function: Callable[_P, object], /, *args: _P.args, **kwargs: _P.kwargs) -> None:
    """Same as addCleanup, except the cleanup items are called even if
    setUpModule fails (unlike tearDownModule).
    """

def doModuleCleanups() -> None:
    """Execute all module cleanup functions. Normally called for you after
    tearDownModule.
    """

if sys.version_info >= (3, 11):
    def enterModuleContext(cm: AbstractContextManager[_T]) -> _T:
        """Same as enterContext, but module-wide."""

def expectedFailure(test_item: _FT) -> _FT: ...
def skip(reason: str) -> Callable[[_FT], _FT]:
    """
    Unconditionally skip a test.
    """

def skipIf(condition: object, reason: str) -> Callable[[_FT], _FT]:
    """
    Skip a test if the condition is true.
    """

def skipUnless(condition: object, reason: str) -> Callable[[_FT], _FT]:
    """
    Skip a test unless the condition is true.
    """

class SkipTest(Exception):
    """
    Raise this exception in a test to skip it.

    Usually you can use TestCase.skipTest() or one of the skipping decorators
    instead of raising this directly.
    """

    def __init__(self, reason: str) -> None: ...

@type_check_only
class _SupportsAbsAndDunderGE(SupportsDunderGE[Any], SupportsAbs[Any], Protocol): ...

class TestCase:
    """A class whose instances are single test cases.

    By default, the test code itself should be placed in a method named
    'runTest'.

    If the fixture may be used for many test cases, create as
    many test methods as are needed. When instantiating such a TestCase
    subclass, specify in the constructor arguments the name of the test method
    that the instance is to execute.

    Test authors should subclass TestCase for their own tests. Construction
    and deconstruction of the test's environment ('fixture') can be
    implemented by overriding the 'setUp' and 'tearDown' methods respectively.

    If it is necessary to override the __init__ method, the base class
    __init__ method must always be called. It is important that subclasses
    should not change the signature of their __init__ method, since instances
    of the classes are instantiated automatically by parts of the framework
    in order to be run.

    When subclassing TestCase, you can set these attributes:
    * failureException: determines which exception will be raised when
        the instance's assertion methods fail; test methods raising this
        exception will be deemed to have 'failed' rather than 'errored'.
    * longMessage: determines whether long messages (including repr of
        objects used in assert methods) will be printed on failure in *addition*
        to any explicit message passed.
    * maxDiff: sets the maximum length of a diff in failure messages
        by assert methods using difflib. It is looked up as an instance
        attribute so can be configured by individual tests if required.
    """

    failureException: type[BaseException]
    longMessage: bool
    maxDiff: int | None
    # undocumented
    _testMethodName: str
    # undocumented
    _testMethodDoc: str
    def __init__(self, methodName: str = "runTest") -> None:
        """Create an instance of the class that will use the named test
        method when executed. Raises a ValueError if the instance does
        not have a method with the specified name.
        """

    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def setUp(self) -> None:
        """Hook method for setting up the test fixture before exercising it."""

    def tearDown(self) -> None:
        """Hook method for deconstructing the test fixture after testing it."""

    @classmethod
    def setUpClass(cls) -> None:
        """Hook method for setting up class fixture before running tests in the class."""

    @classmethod
    def tearDownClass(cls) -> None:
        """Hook method for deconstructing the class fixture after running all tests in the class."""

    def run(self, result: unittest.result.TestResult | None = None) -> unittest.result.TestResult | None: ...
    def __call__(self, result: unittest.result.TestResult | None = ...) -> unittest.result.TestResult | None: ...
    def skipTest(self, reason: Any) -> NoReturn:
        """Skip this test."""

    def subTest(self, msg: Any = ..., **params: Any) -> AbstractContextManager[None]:
        """Return a context manager that will return the enclosed block
        of code in a subtest identified by the optional message and
        keyword parameters.  A failure in the subtest marks the test
        case as failed but resumes execution at the end of the enclosed
        block, allowing further test code to be executed.
        """

    def debug(self) -> None:
        """Run the test without collecting errors in a TestResult"""
    if sys.version_info < (3, 11):
        def _addSkip(self, result: unittest.result.TestResult, test_case: TestCase, reason: str) -> None: ...

    def assertEqual(self, first: Any, second: Any, msg: Any = None) -> None:
        """Fail if the two objects are unequal as determined by the '=='
        operator.
        """

    def assertNotEqual(self, first: Any, second: Any, msg: Any = None) -> None:
        """Fail if the two objects are equal as determined by the '!='
        operator.
        """

    def assertTrue(self, expr: Any, msg: Any = None) -> None:
        """Check that the expression is true."""

    def assertFalse(self, expr: Any, msg: Any = None) -> None:
        """Check that the expression is false."""

    def assertIs(self, expr1: object, expr2: object, msg: Any = None) -> None:
        """Just like self.assertTrue(a is b), but with a nicer default message."""

    def assertIsNot(self, expr1: object, expr2: object, msg: Any = None) -> None:
        """Just like self.assertTrue(a is not b), but with a nicer default message."""

    def assertIsNone(self, obj: object, msg: Any = None) -> None:
        """Same as self.assertTrue(obj is None), with a nicer default message."""

    def assertIsNotNone(self, obj: object, msg: Any = None) -> None:
        """Included for symmetry with assertIsNone."""

    def assertIn(self, member: Any, container: Iterable[Any] | Container[Any], msg: Any = None) -> None:
        """Just like self.assertTrue(a in b), but with a nicer default message."""

    def assertNotIn(self, member: Any, container: Iterable[Any] | Container[Any], msg: Any = None) -> None:
        """Just like self.assertTrue(a not in b), but with a nicer default message."""

    def assertIsInstance(self, obj: object, cls: _ClassInfo, msg: Any = None) -> None:
        """Same as self.assertTrue(isinstance(obj, cls)), with a nicer
        default message.
        """

    def assertNotIsInstance(self, obj: object, cls: _ClassInfo, msg: Any = None) -> None:
        """Included for symmetry with assertIsInstance."""

    @overload
    def assertGreater(self, a: SupportsDunderGT[_T], b: _T, msg: Any = None) -> None:
        """Just like self.assertTrue(a > b), but with a nicer default message."""

    @overload
    def assertGreater(self, a: _T, b: SupportsDunderLT[_T], msg: Any = None) -> None: ...
    @overload
    def assertGreaterEqual(self, a: SupportsDunderGE[_T], b: _T, msg: Any = None) -> None:
        """Just like self.assertTrue(a >= b), but with a nicer default message."""

    @overload
    def assertGreaterEqual(self, a: _T, b: SupportsDunderLE[_T], msg: Any = None) -> None: ...
    @overload
    def assertLess(self, a: SupportsDunderLT[_T], b: _T, msg: Any = None) -> None:
        """Just like self.assertTrue(a < b), but with a nicer default message."""

    @overload
    def assertLess(self, a: _T, b: SupportsDunderGT[_T], msg: Any = None) -> None: ...
    @overload
    def assertLessEqual(self, a: SupportsDunderLE[_T], b: _T, msg: Any = None) -> None:
        """Just like self.assertTrue(a <= b), but with a nicer default message."""

    @overload
    def assertLessEqual(self, a: _T, b: SupportsDunderGE[_T], msg: Any = None) -> None: ...
    # `assertRaises`, `assertRaisesRegex`, and `assertRaisesRegexp`
    # are not using `ParamSpec` intentionally,
    # because they might be used with explicitly wrong arg types to raise some error in tests.
    @overload
    def assertRaises(
        self,
        expected_exception: type[BaseException] | tuple[type[BaseException], ...],
        callable: Callable[..., object],
        *args: Any,
        **kwargs: Any,
    ) -> None:
        """Fail unless an exception of class expected_exception is raised
        by the callable when invoked with specified positional and
        keyword arguments. If a different type of exception is
        raised, it will not be caught, and the test case will be
        deemed to have suffered an error, exactly as for an
        unexpected exception.

        If called with the callable and arguments omitted, will return a
        context object used like this::

             with self.assertRaises(SomeException):
                 do_something()

        An optional keyword argument 'msg' can be provided when assertRaises
        is used as a context object.

        The context manager keeps a reference to the exception as
        the 'exception' attribute. This allows you to inspect the
        exception after the assertion::

            with self.assertRaises(SomeException) as cm:
                do_something()
            the_exception = cm.exception
            self.assertEqual(the_exception.error_code, 3)
        """

    @overload
    def assertRaises(
        self, expected_exception: type[_E] | tuple[type[_E], ...], *, msg: Any = ...
    ) -> _AssertRaisesContext[_E]: ...
    @overload
    def assertRaisesRegex(
        self,
        expected_exception: type[BaseException] | tuple[type[BaseException], ...],
        expected_regex: str | Pattern[str],
        callable: Callable[..., object],
        *args: Any,
        **kwargs: Any,
    ) -> None:
        """Asserts that the message in a raised exception matches a regex.

        Args:
            expected_exception: Exception class expected to be raised.
            expected_regex: Regex (re.Pattern object or string) expected
                    to be found in error message.
            args: Function to be called and extra positional args.
            kwargs: Extra kwargs.
            msg: Optional message used in case of failure. Can only be used
                    when assertRaisesRegex is used as a context manager.
        """

    @overload
    def assertRaisesRegex(
        self, expected_exception: type[_E] | tuple[type[_E], ...], expected_regex: str | Pattern[str], *, msg: Any = ...
    ) -> _AssertRaisesContext[_E]: ...
    @overload
    def assertWarns(
        self,
        expected_warning: type[Warning] | tuple[type[Warning], ...],
        callable: Callable[_P, object],
        *args: _P.args,
        **kwargs: _P.kwargs,
    ) -> None:
        """Fail unless a warning of class warnClass is triggered
        by the callable when invoked with specified positional and
        keyword arguments.  If a different type of warning is
        triggered, it will not be handled: depending on the other
        warning filtering rules in effect, it might be silenced, printed
        out, or raised as an exception.

        If called with the callable and arguments omitted, will return a
        context object used like this::

             with self.assertWarns(SomeWarning):
                 do_something()

        An optional keyword argument 'msg' can be provided when assertWarns
        is used as a context object.

        The context manager keeps a reference to the first matching
        warning as the 'warning' attribute; similarly, the 'filename'
        and 'lineno' attributes give you information about the line
        of Python code from which the warning was triggered.
        This allows you to inspect the warning after the assertion::

            with self.assertWarns(SomeWarning) as cm:
                do_something()
            the_warning = cm.warning
            self.assertEqual(the_warning.some_attribute, 147)
        """

    @overload
    def assertWarns(
        self, expected_warning: type[Warning] | tuple[type[Warning], ...], *, msg: Any = ...
    ) -> _AssertWarnsContext: ...
    @overload
    def assertWarnsRegex(
        self,
        expected_warning: type[Warning] | tuple[type[Warning], ...],
        expected_regex: str | Pattern[str],
        callable: Callable[_P, object],
        *args: _P.args,
        **kwargs: _P.kwargs,
    ) -> None:
        """Asserts that the message in a triggered warning matches a regexp.
        Basic functioning is similar to assertWarns() with the addition
        that only warnings whose messages also match the regular expression
        are considered successful matches.

        Args:
            expected_warning: Warning class expected to be triggered.
            expected_regex: Regex (re.Pattern object or string) expected
                    to be found in error message.
            args: Function to be called and extra positional args.
            kwargs: Extra kwargs.
            msg: Optional message used in case of failure. Can only be used
                    when assertWarnsRegex is used as a context manager.
        """

    @overload
    def assertWarnsRegex(
        self, expected_warning: type[Warning] | tuple[type[Warning], ...], expected_regex: str | Pattern[str], *, msg: Any = ...
    ) -> _AssertWarnsContext: ...
    def assertLogs(
        self, logger: str | logging.Logger | None = None, level: int | str | None = None
    ) -> _AssertLogsContext[_LoggingWatcher]:
        """Fail unless a log message of level *level* or higher is emitted
        on *logger_name* or its children.  If omitted, *level* defaults to
        INFO and *logger* defaults to the root logger.

        This method must be used as a context manager, and will yield
        a recording object with two attributes: `output` and `records`.
        At the end of the context manager, the `output` attribute will
        be a list of the matching formatted log messages and the
        `records` attribute will be a list of the corresponding LogRecord
        objects.

        Example::

            with self.assertLogs('foo', level='INFO') as cm:
                logging.getLogger('foo').info('first message')
                logging.getLogger('foo.bar').error('second message')
            self.assertEqual(cm.output, ['INFO:foo:first message',
                                         'ERROR:foo.bar:second message'])
        """
    if sys.version_info >= (3, 10):
        def assertNoLogs(
            self, logger: str | logging.Logger | None = None, level: int | str | None = None
        ) -> _AssertLogsContext[None]:
            """Fail unless no log messages of level *level* or higher are emitted
            on *logger_name* or its children.

            This method must be used as a context manager.
            """

    @overload
    def assertAlmostEqual(self, first: _S, second: _S, places: None, msg: Any, delta: _SupportsAbsAndDunderGE) -> None:
        """Fail if the two objects are unequal as determined by their
        difference rounded to the given number of decimal places
        (default 7) and comparing to zero, or by comparing that the
        difference between the two objects is more than the given
        delta.

        Note that decimal places (from zero) are usually not the same
        as significant digits (measured from the most significant digit).

        If the two objects compare equal then they will automatically
        compare almost equal.
        """

    @overload
    def assertAlmostEqual(
        self, first: _S, second: _S, places: None = None, msg: Any = None, *, delta: _SupportsAbsAndDunderGE
    ) -> None: ...
    @overload
    def assertAlmostEqual(
        self,
        first: SupportsSub[_T, SupportsAbs[SupportsRound[object]]],
        second: _T,
        places: int | None = None,
        msg: Any = None,
        delta: None = None,
    ) -> None: ...
    @overload
    def assertAlmostEqual(
        self,
        first: _T,
        second: SupportsRSub[_T, SupportsAbs[SupportsRound[object]]],
        places: int | None = None,
        msg: Any = None,
        delta: None = None,
    ) -> None: ...
    @overload
    def assertNotAlmostEqual(self, first: _S, second: _S, places: None, msg: Any, delta: _SupportsAbsAndDunderGE) -> None:
        """Fail if the two objects are equal as determined by their
        difference rounded to the given number of decimal places
        (default 7) and comparing to zero, or by comparing that the
        difference between the two objects is less than the given delta.

        Note that decimal places (from zero) are usually not the same
        as significant digits (measured from the most significant digit).

        Objects that are equal automatically fail.
        """

    @overload
    def assertNotAlmostEqual(
        self, first: _S, second: _S, places: None = None, msg: Any = None, *, delta: _SupportsAbsAndDunderGE
    ) -> None: ...
    @overload
    def assertNotAlmostEqual(
        self,
        first: SupportsSub[_T, SupportsAbs[SupportsRound[object]]],
        second: _T,
        places: int | None = None,
        msg: Any = None,
        delta: None = None,
    ) -> None: ...
    @overload
    def assertNotAlmostEqual(
        self,
        first: _T,
        second: SupportsRSub[_T, SupportsAbs[SupportsRound[object]]],
        places: int | None = None,
        msg: Any = None,
        delta: None = None,
    ) -> None: ...
    def assertRegex(self, text: AnyStr, expected_regex: AnyStr | Pattern[AnyStr], msg: Any = None) -> None:
        """Fail the test unless the text matches the regular expression."""

    def assertNotRegex(self, text: AnyStr, unexpected_regex: AnyStr | Pattern[AnyStr], msg: Any = None) -> None:
        """Fail the test if the text matches the regular expression."""

    def assertCountEqual(self, first: Iterable[Any], second: Iterable[Any], msg: Any = None) -> None:
        """Asserts that two iterables have the same elements, the same number of
        times, without regard to order.

            self.assertEqual(Counter(list(first)),
                             Counter(list(second)))

         Example:
            - [0, 1, 1] and [1, 0, 1] compare equal.
            - [0, 0, 1] and [0, 1] compare unequal.

        """

    def addTypeEqualityFunc(self, typeobj: type[Any], function: Callable[..., None]) -> None:
        """Add a type specific assertEqual style function to compare a type.

        This method is for use by TestCase subclasses that need to register
        their own type equality functions to provide nicer error messages.

        Args:
            typeobj: The data type to call this function on when both values
                    are of the same type in assertEqual().
            function: The callable taking two arguments and an optional
                    msg= argument that raises self.failureException with a
                    useful error message when the two arguments are not equal.
        """

    def assertMultiLineEqual(self, first: str, second: str, msg: Any = None) -> None:
        """Assert that two multi-line strings are equal."""

    def assertSequenceEqual(
        self, seq1: Sequence[Any], seq2: Sequence[Any], msg: Any = None, seq_type: type[Sequence[Any]] | None = None
    ) -> None:
        """An equality assertion for ordered sequences (like lists and tuples).

        For the purposes of this function, a valid ordered sequence type is one
        which can be indexed, has a length, and has an equality operator.

        Args:
            seq1: The first sequence to compare.
            seq2: The second sequence to compare.
            seq_type: The expected datatype of the sequences, or None if no
                    datatype should be enforced.
            msg: Optional message to use on failure instead of a list of
                    differences.
        """

    def assertListEqual(self, list1: list[Any], list2: list[Any], msg: Any = None) -> None:
        """A list-specific equality assertion.

        Args:
            list1: The first list to compare.
            list2: The second list to compare.
            msg: Optional message to use on failure instead of a list of
                    differences.

        """

    def assertTupleEqual(self, tuple1: tuple[Any, ...], tuple2: tuple[Any, ...], msg: Any = None) -> None:
        """A tuple-specific equality assertion.

        Args:
            tuple1: The first tuple to compare.
            tuple2: The second tuple to compare.
            msg: Optional message to use on failure instead of a list of
                    differences.
        """

    def assertSetEqual(self, set1: AbstractSet[object], set2: AbstractSet[object], msg: Any = None) -> None:
        """A set-specific equality assertion.

        Args:
            set1: The first set to compare.
            set2: The second set to compare.
            msg: Optional message to use on failure instead of a list of
                    differences.

        assertSetEqual uses ducktyping to support different types of sets, and
        is optimized for sets specifically (parameters must support a
        difference method).
        """
    # assertDictEqual accepts only true dict instances. We can't use that here, since that would make
    # assertDictEqual incompatible with TypedDict.
    def assertDictEqual(self, d1: Mapping[Any, object], d2: Mapping[Any, object], msg: Any = None) -> None: ...
    def fail(self, msg: Any = None) -> NoReturn:
        """Fail immediately, with the given message."""

    def countTestCases(self) -> int: ...
    def defaultTestResult(self) -> unittest.result.TestResult: ...
    def id(self) -> str: ...
    def shortDescription(self) -> str | None:
        """Returns a one-line description of the test, or None if no
        description has been provided.

        The default implementation of this method returns the first line of
        the specified test method's docstring.
        """

    def addCleanup(self, function: Callable[_P, object], /, *args: _P.args, **kwargs: _P.kwargs) -> None:
        """Add a function, with arguments, to be called when the test is
        completed. Functions added are called on a LIFO basis and are
        called after tearDown on test failure or success.

        Cleanup items are called even if setUp fails (unlike tearDown).
        """
    if sys.version_info >= (3, 11):
        def enterContext(self, cm: AbstractContextManager[_T]) -> _T:
            """Enters the supplied context manager.

            If successful, also adds its __exit__ method as a cleanup
            function and returns the result of the __enter__ method.
            """

    def doCleanups(self) -> None:
        """Execute all cleanup functions. Normally called for you after
        tearDown.
        """

    @classmethod
    def addClassCleanup(cls, function: Callable[_P, object], /, *args: _P.args, **kwargs: _P.kwargs) -> None:
        """Same as addCleanup, except the cleanup items are called even if
        setUpClass fails (unlike tearDownClass).
        """

    @classmethod
    def doClassCleanups(cls) -> None:
        """Execute all class cleanup functions. Normally called for you after
        tearDownClass.
        """
    if sys.version_info >= (3, 11):
        @classmethod
        def enterClassContext(cls, cm: AbstractContextManager[_T]) -> _T:
            """Same as enterContext, but class-wide."""

    def _formatMessage(self, msg: str | None, standardMsg: str) -> str:  # undocumented
        """Honour the longMessage attribute when generating failure messages.
        If longMessage is False this means:
        * Use only an explicit message if it is provided
        * Otherwise use the standard message for the assert

        If longMessage is True:
        * Use the standard message
        * If an explicit message is provided, plus ' : ' and the explicit message
        """

    def _getAssertEqualityFunc(self, first: Any, second: Any) -> Callable[..., None]:  # undocumented
        """Get a detailed comparison function for the types of the two args.

        Returns: A callable accepting (first, second, msg=None) that will
        raise a failure exception if first != second with a useful human
        readable error message for those types.
        """
    if sys.version_info < (3, 12):
        failUnlessEqual = assertEqual
        assertEquals = assertEqual
        failIfEqual = assertNotEqual
        assertNotEquals = assertNotEqual
        failUnless = assertTrue
        assert_ = assertTrue
        failIf = assertFalse
        failUnlessRaises = assertRaises
        failUnlessAlmostEqual = assertAlmostEqual
        assertAlmostEquals = assertAlmostEqual
        failIfAlmostEqual = assertNotAlmostEqual
        assertNotAlmostEquals = assertNotAlmostEqual
        assertRegexpMatches = assertRegex
        assertNotRegexpMatches = assertNotRegex
        assertRaisesRegexp = assertRaisesRegex
        def assertDictContainsSubset(self, subset: Mapping[Any, Any], dictionary: Mapping[Any, Any], msg: object = None) -> None:
            """Checks whether dictionary is a superset of subset."""
    if sys.version_info >= (3, 10):
        # Runtime has *args, **kwargs, but will error if any are supplied
        def __init_subclass__(cls, *args: Never, **kwargs: Never) -> None: ...

    if sys.version_info >= (3, 14):
        def assertIsSubclass(self, cls: type, superclass: type | tuple[type, ...], msg: Any = None) -> None: ...
        def assertNotIsSubclass(self, cls: type, superclass: type | tuple[type, ...], msg: Any = None) -> None: ...
        def assertHasAttr(self, obj: object, name: str, msg: Any = None) -> None: ...
        def assertNotHasAttr(self, obj: object, name: str, msg: Any = None) -> None: ...
        def assertStartsWith(self, s: _SB, prefix: _SB | tuple[_SB, ...], msg: Any = None) -> None: ...
        def assertNotStartsWith(self, s: _SB, prefix: _SB | tuple[_SB, ...], msg: Any = None) -> None: ...
        def assertEndsWith(self, s: _SB, suffix: _SB | tuple[_SB, ...], msg: Any = None) -> None: ...
        def assertNotEndsWith(self, s: _SB, suffix: _SB | tuple[_SB, ...], msg: Any = None) -> None: ...

class FunctionTestCase(TestCase):
    """A test case that wraps a test function.

    This is useful for slipping pre-existing test functions into the
    unittest framework. Optionally, set-up and tidy-up functions can be
    supplied. As with TestCase, the tidy-up ('tearDown') function will
    always be called if the set-up ('setUp') function ran successfully.
    """

    def __init__(
        self,
        testFunc: Callable[[], object],
        setUp: Callable[[], object] | None = None,
        tearDown: Callable[[], object] | None = None,
        description: str | None = None,
    ) -> None: ...
    def runTest(self) -> None: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

class _AssertRaisesContext(_AssertRaisesBaseContext, Generic[_E]):
    """A context manager used to implement TestCase.assertRaises* methods."""

    exception: _E
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, tb: TracebackType | None
    ) -> bool: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

class _AssertWarnsContext(_AssertRaisesBaseContext):
    """A context manager used to implement TestCase.assertWarns* methods."""

    warning: WarningMessage
    filename: str
    lineno: int
    warnings: list[WarningMessage]
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, tb: TracebackType | None
    ) -> None: ...
