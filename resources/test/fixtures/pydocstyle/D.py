# No docstring, so we can test D100
from functools import wraps
import os
from .expected import Expectation
from typing import overload
from typing_extensions import override


expectation = Expectation()
expect = expectation.expect

expect('class_', 'D101: Missing docstring in public class')


class class_:

    expect('meta', 'D419: Docstring is empty')

    class meta:
        """"""

    @expect('D102: Missing docstring in public method')
    def method(self=None):
        pass

    def _ok_since_private(self=None):
        pass

    @overload
    def overloaded_method(self, a: int) -> str:
        ...

    @overload
    def overloaded_method(self, a: str) -> str:
        """Foo bar documentation."""
        ...

    def overloaded_method(a):
        """Foo bar documentation."""
        return str(a)

    expect('overloaded_method',
           "D418: Function/ Method decorated with @overload"
           " shouldn't contain a docstring")

    @override
    def overridden_method(a):
        return str(a)

    @property
    def foo(self):
        """The foo of the thing, which isn't in imperative mood."""
        return "hello"

    @expect('D102: Missing docstring in public method')
    def __new__(self=None):
        pass

    @expect('D107: Missing docstring in __init__')
    def __init__(self=None):
        pass

    @expect('D105: Missing docstring in magic method')
    def __str__(self=None):
        pass

    @expect('D102: Missing docstring in public method')
    def __call__(self=None, x=None, y=None, z=None):
        pass


@expect('D419: Docstring is empty')
def function():
    """ """
    def ok_since_nested():
        pass

    @expect('D419: Docstring is empty')
    def nested():
        ''


def function_with_nesting():
    """Foo bar documentation."""
    @overload
    def nested_overloaded_func(a: int) -> str:
        ...

    @overload
    def nested_overloaded_func(a: str) -> str:
        """Foo bar documentation."""
        ...

    def nested_overloaded_func(a):
        """Foo bar documentation."""
        return str(a)


expect('nested_overloaded_func',
       "D418: Function/ Method decorated with @overload"
       " shouldn't contain a docstring")


@overload
def overloaded_func(a: int) -> str:
    ...


@overload
def overloaded_func(a: str) -> str:
    """Foo bar documentation."""
    ...


def overloaded_func(a):
    """Foo bar documentation."""
    return str(a)


expect('overloaded_func',
       "D418: Function/ Method decorated with @overload"
       " shouldn't contain a docstring")


@expect('D200: One-line docstring should fit on one line with quotes '
        '(found 3)')
@expect('D212: Multi-line docstring summary should start at the first line')
def asdlkfasd():
    """
    Wrong.
    """


@expect('D201: No blank lines allowed before function docstring (found 1)')
def leading_space():

    """Leading space."""


@expect('D202: No blank lines allowed after function docstring (found 1)')
def trailing_space():
    """Leading space."""

    pass


@expect('D201: No blank lines allowed before function docstring (found 1)')
@expect('D202: No blank lines allowed after function docstring (found 1)')
def trailing_and_leading_space():

    """Trailing and leading space."""

    pass


expect('LeadingSpaceMissing',
       'D203: 1 blank line required before class docstring (found 0)')


class LeadingSpaceMissing:
    """Leading space missing."""


expect('WithLeadingSpace',
       'D211: No blank lines allowed before class docstring (found 1)')


class WithLeadingSpace:

    """With leading space."""


expect('TrailingSpace',
       'D204: 1 blank line required after class docstring (found 0)')
expect('TrailingSpace',
       'D211: No blank lines allowed before class docstring (found 1)')


class TrailingSpace:

    """TrailingSpace."""
    pass


expect('LeadingAndTrailingSpaceMissing',
       'D203: 1 blank line required before class docstring (found 0)')
expect('LeadingAndTrailingSpaceMissing',
       'D204: 1 blank line required after class docstring (found 0)')


class LeadingAndTrailingSpaceMissing:
    """Leading and trailing space missing."""
    pass


@expect('D205: 1 blank line required between summary line and description '
        '(found 0)')
@expect('D213: Multi-line docstring summary should start at the second line')
def multi_line_zero_separating_blanks():
    """Summary.
    Description.

    """


@expect('D205: 1 blank line required between summary line and description '
        '(found 2)')
@expect('D213: Multi-line docstring summary should start at the second line')
def multi_line_two_separating_blanks():
    """Summary.


    Description.

    """


@expect('D213: Multi-line docstring summary should start at the second line')
def multi_line_one_separating_blanks():
    """Summary.

    Description.

    """


@expect('D207: Docstring is under-indented')
@expect('D213: Multi-line docstring summary should start at the second line')
def asdfsdf():
    """Summary.

Description.

    """


@expect('D207: Docstring is under-indented')
@expect('D213: Multi-line docstring summary should start at the second line')
def asdsdfsdffsdf():
    """Summary.

    Description.

"""


@expect('D208: Docstring is over-indented')
@expect('D213: Multi-line docstring summary should start at the second line')
def asdfsdsdf24():
    """Summary.

       Description.

    """


@expect('D208: Docstring is over-indented')
@expect('D213: Multi-line docstring summary should start at the second line')
def asdfsdsdfsdf24():
    """Summary.

    Description.

        """


@expect('D208: Docstring is over-indented')
@expect('D213: Multi-line docstring summary should start at the second line')
def asdfsdfsdsdsdfsdf24():
    """Summary.

        Description.

    """


@expect('D209: Multi-line docstring closing quotes should be on a separate '
        'line')
@expect('D213: Multi-line docstring summary should start at the second line')
def asdfljdf24():
    """Summary.

    Description."""


@expect('D210: No whitespaces allowed surrounding docstring text')
def endswith():
    """Whitespace at the end. """


@expect('D210: No whitespaces allowed surrounding docstring text')
def around():
    """ Whitespace at everywhere. """


@expect('D210: No whitespaces allowed surrounding docstring text')
@expect('D213: Multi-line docstring summary should start at the second line')
def multiline():
    """ Whitespace at the beginning.

    This is the end.
    """


@expect('D300: Use """triple double quotes""" (found \'\'\'-quotes)')
def triple_single_quotes_raw():
    r'''Summary.'''


@expect('D300: Use """triple double quotes""" (found \'\'\'-quotes)')
def triple_single_quotes_raw_uppercase():
    R'''Summary.'''


@expect('D300: Use """triple double quotes""" (found \'-quotes)')
def single_quotes_raw():
    r'Summary.'


@expect('D300: Use """triple double quotes""" (found \'-quotes)')
def single_quotes_raw_uppercase():
    R'Summary.'


@expect('D300: Use """triple double quotes""" (found \'-quotes)')
@expect('D301: Use r""" if any backslashes in a docstring')
def single_quotes_raw_uppercase_backslash():
    R'Sum\mary.'


@expect('D301: Use r""" if any backslashes in a docstring')
def double_quotes_backslash():
    """Sum\\mary."""


@expect('D301: Use r""" if any backslashes in a docstring')
def double_quotes_backslash_uppercase():
    R"""Sum\\mary."""


@expect('D213: Multi-line docstring summary should start at the second line')
def exceptions_of_D301():
    """Exclude some backslashes from D301.

    In particular, line continuations \
    and unicode literals \u0394 and \N{GREEK CAPITAL LETTER DELTA}.
    They are considered to be intentionally unescaped.
    """


@expect("D400: First line should end with a period (not 'y')")
@expect("D415: First line should end with a period, question mark, "
        "or exclamation point (not 'y')")
def lwnlkjl():
    """Summary"""


@expect("D401: First line should be in imperative mood "
        "(perhaps 'Return', not 'Returns')")
def liouiwnlkjl():
    """Returns foo."""


@expect("D401: First line should be in imperative mood; try rephrasing "
        "(found 'Constructor')")
def sdgfsdg23245():
    """Constructor for a foo."""


@expect("D401: First line should be in imperative mood; try rephrasing "
        "(found 'Constructor')")
def sdgfsdg23245777():
    """Constructor."""


@expect('D402: First line should not be the function\'s "signature"')
def foobar():
    """Signature: foobar()."""


@expect('D213: Multi-line docstring summary should start at the second line')
def new_209():
    """First line.

    More lines.
    """
    pass


@expect('D213: Multi-line docstring summary should start at the second line')
def old_209():
    """One liner.

    Multi-line comments. OK to have extra blank line

    """


@expect("D103: Missing docstring in public function")
def oneliner_d102(): return


@expect("D400: First line should end with a period (not 'r')")
@expect("D415: First line should end with a period, question mark,"
        " or exclamation point (not 'r')")
def oneliner_withdoc(): """One liner"""


def ignored_decorator(func):   # noqa: D400,D401,D415
    """Runs something"""
    func()
    pass


def decorator_for_test(func):   # noqa: D400,D401,D415
    """Runs something"""
    func()
    pass


@ignored_decorator
def oneliner_ignored_decorator(): """One liner"""


@decorator_for_test
@expect("D400: First line should end with a period (not 'r')")
@expect("D415: First line should end with a period, question mark,"
        " or exclamation point (not 'r')")
def oneliner_with_decorator_expecting_errors(): """One liner"""


@decorator_for_test
def valid_oneliner_with_decorator(): """One liner."""


@expect("D207: Docstring is under-indented")
@expect('D213: Multi-line docstring summary should start at the second line')
def docstring_start_in_same_line(): """First Line.

    Second Line
    """


def function_with_lambda_arg(x=lambda y: y):
    """Wrap the given lambda."""


@expect('D213: Multi-line docstring summary should start at the second line')
def a_following_valid_function(x=None):
    """Check for a bug where the previous function caused an assertion.

    The assertion was caused in the next function, so this one is necessary.

    """


def outer_function():
    """Do something."""
    def inner_function():
        """Do inner something."""
        return 0


@expect("D400: First line should end with a period (not 'g')")
@expect("D401: First line should be in imperative mood "
        "(perhaps 'Run', not 'Runs')")
@expect("D415: First line should end with a period, question mark, "
        "or exclamation point (not 'g')")
def docstring_bad():
    """Runs something"""
    pass


def docstring_bad_ignore_all():  # noqa
    """Runs something"""
    pass


def docstring_bad_ignore_one():  # noqa: D400,D401,D415
    """Runs something"""
    pass


@expect("D401: First line should be in imperative mood "
        "(perhaps 'Run', not 'Runs')")
def docstring_ignore_some_violations_but_catch_D401():  # noqa: E501,D400,D415
    """Runs something"""
    pass


@expect(
    "D401: First line should be in imperative mood "
    "(perhaps 'Initiate', not 'Initiates')"
)
def docstring_initiates():
    """Initiates the process."""


@expect(
    "D401: First line should be in imperative mood "
    "(perhaps 'Initialize', not 'Initializes')"
)
def docstring_initializes():
    """Initializes the process."""


@wraps(docstring_bad_ignore_one)
def bad_decorated_function():
    """Bad (E501) but decorated"""
    pass


def valid_google_string():  # noqa: D400
    """Test a valid something!"""


@expect("D415: First line should end with a period, question mark, "
        "or exclamation point (not 'g')")
def bad_google_string():  # noqa: D400
    """Test a valid something"""


# This is reproducing a bug where AttributeError is raised when parsing class
# parameters as functions for Google / Numpy conventions.
class Blah:  # noqa: D203,D213
    """A Blah.

    Parameters
    ----------
    x : int

    """

    def __init__(self, x):
        pass


expect(os.path.normcase(__file__ if __file__[-1] != 'c' else __file__[:-1]),
       'D100: Missing docstring in public module')


@expect('D201: No blank lines allowed before function docstring (found 1)')
@expect('D213: Multi-line docstring summary should start at the second line')
def multiline_leading_space():

    """Leading space.

    More content.
    """


@expect('D202: No blank lines allowed after function docstring (found 1)')
@expect('D213: Multi-line docstring summary should start at the second line')
def multiline_trailing_space():
    """Leading space.

    More content.
    """

    pass


@expect('D201: No blank lines allowed before function docstring (found 1)')
@expect('D202: No blank lines allowed after function docstring (found 1)')
@expect('D213: Multi-line docstring summary should start at the second line')
def multiline_trailing_and_leading_space():

    """Trailing and leading space.

    More content.
    """

    pass


@expect('D210: No whitespaces allowed surrounding docstring text')
@expect("D400: First line should end with a period (not '\"')")
@expect("D415: First line should end with a period, question mark, "
        "or exclamation point (not '\"')")
def endswith_quote():
    """Whitespace at the end, but also a quote" """
