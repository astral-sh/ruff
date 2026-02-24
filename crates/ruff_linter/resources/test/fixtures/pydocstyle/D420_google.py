"""Tests for D420 (section order) with Google convention."""


def correct_order():
    """Summary.

    Args:
        x: Description.

    Keyword Args:
        y: Description.

    Returns:
        int

    Yields:
        int

    Raises:
        ValueError

    Notes:
        Some notes.

    Examples:
        >>> correct_order()
    """


def returns_before_args():
    """Summary.

    Returns:
        int

    Args:
        x: Description.
    """


def raises_before_returns():
    """Summary.

    Raises:
        ValueError

    Returns:
        int
    """


def raises_before_args():
    """Summary.

    Raises:
        ValueError

    Args:
        x: Description.

    Returns:
        int
    """


def single_section():
    """Summary.

    Returns:
        int
    """


def correct_args_order():
    """Summary.

    Args:
        x: Description.

    Keyword Args:
        y: Description.

    Returns:
        int
    """


# All arg-like sections share position 0 — no ordering enforced among them.
def keyword_args_before_args():
    """Summary.

    Keyword Args:
        y: Description.

    Args:
        x: Description.
    """


def arguments_alias():
    """Summary.

    Returns:
        int

    Arguments:
        x: Description.
    """


# Non-core sections (Notes, Examples, Attributes, etc.) are unordered.
# No diagnostic expected for any ordering among them.
def notes_before_raises_unordered():
    """Summary.

    Notes:
        Some notes.

    Raises:
        ValueError
    """


def unordered_sections_only():
    """Summary.

    Examples:
        >>> func()

    Notes:
        Some notes.
    """


def examples_before_returns():
    """Summary.

    Examples:
        >>> func()

    Returns:
        int
    """


def returns_then_unordered_then_args():
    """Summary.

    Returns:
        int

    Notes:
        Some notes.

    Args:
        x: Description.
    """


class CorrectClassDocstring:
    """Summary.

    Attributes:
        x: Description.

    Examples:
        >>> obj = CorrectClassDocstring()
    """


# Attributes and Examples are both unordered — no diagnostic.
class UnorderedClassDocstring:
    """Summary.

    Examples:
        >>> obj = UnorderedClassDocstring()

    Attributes:
        x: Description.
    """
