"""This module docstring does not need to be written in imperative mood."""

from functools import cached_property

# Bad examples

def bad_liouiwnlkjl():
    """Returns foo."""


def bad_sdgfsdg23245():
    """Constructor for a foo."""


def bad_sdgfsdg23245777():
    """

    Constructor for a boa.

    """


def bad_run_something():
    """Runs something"""

    def bad_nested():
        """Runs other things, nested"""

    bad_nested()


def multi_line():
    """Writes a logical line that
    extends to two physical lines.
    """


# Good examples

def good_run_something():
    """Run away."""

    def good_nested():
        """Run to the hills."""

    good_nested()


def good_construct():
    """Construct a beautiful house."""


def good_multi_line():
    """Write a logical line that
    extends to two physical lines.
    """


good_top_level_var = False
"""This top level assignment attribute docstring does not need to be written in imperative mood."""


# Classes and their members

class Thingy:
    """This class docstring does not need to be written in imperative mood."""

    _beep = "boop"
    """This class attribute docstring does not need to be written in imperative mood."""

    def bad_method(self):
        """This method docstring should be written in imperative mood."""

    @property
    def good_property(self):
        """This property method docstring does not need to be written in imperative mood."""
        return self._beep

    @cached_property
    def good_cached_property(self):
        """This property method docstring does not need to be written in imperative mood."""
        return 42 * 42

    class NestedThingy:
        """This nested class docstring does not need to be written in imperative mood."""


# Test functions

def test_something():
    """This test function does not need to be written in imperative mood.

    pydocstyle's rationale:
    We exclude tests from the imperative mood check, because to phrase
    their docstring in the imperative mood, they would have to start with
    a highly redundant "Test that ..."
    """


def runTest():
    """This test function does not need to be written in imperative mood, either."""
