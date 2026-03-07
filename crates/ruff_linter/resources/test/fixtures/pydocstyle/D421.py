"""Module docstring."""

from functools import cached_property


class Foo:
    # Bad: property docstrings starting with getter/return verbs

    @property
    def bar(self) -> str:
        """Returns the bar."""
        return self._bar

    @property
    def baz(self) -> str:
        """Return the baz."""
        return self._baz

    @property
    def qux(self) -> str:
        """Gets the qux."""
        return self._qux

    @property
    def quux(self) -> str:
        """Get the quux."""
        return self._quux

    @property
    def corge(self) -> str:
        """Yields the corge."""
        return self._corge

    @property
    def grault(self) -> str:
        """Yield the grault."""
        return self._grault

    @property
    def garply(self) -> str:
        """Fetches the garply."""
        return self._garply

    @property
    def waldo(self) -> str:
        """Fetch the waldo."""
        return self._waldo

    @property
    def fred(self) -> str:
        """Retrieves the fred."""
        return self._fred

    @property
    def plugh(self) -> str:
        """Retrieve the plugh."""
        return self._plugh

    # Bad: multiline property docstring starting with a verb

    @property
    def multiline_bad(self) -> str:
        """Returns the value.

        More detail here.
        """
        return self._value

    # Good: attribute-style property docstrings

    @property
    def good_attr_style(self) -> str:
        """The bar value."""
        return self._bar

    @property
    def good_noun(self) -> str:
        """A description of something."""
        return self._something

    @property
    def good_multiline(self) -> str:
        """The current value.

        More detail here.
        """
        return self._value

    @cached_property
    def good_cached(self) -> int:
        """The cached result."""
        return 42


# Regular (non-property) methods starting with "Returns" should NOT be flagged

class Bar:
    def regular_method(self) -> str:
        """Returns the bar value."""
        return self._bar

    def another_method(self) -> str:
        """Gets the value."""
        return self._value


def top_level_function() -> str:
    """Returns a value."""
    return "value"
