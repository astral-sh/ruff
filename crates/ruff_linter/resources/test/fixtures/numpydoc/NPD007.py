"""Test cases for NPD007 (numpydoc GL07) - section ordering."""


def correct_order():
    """Calculate something.

    Parameters
    ----------
    x : int
        Input value.

    Returns
    -------
    int
        Output value.

    Examples
    --------
    >>> correct_order()
    42
    """
    return 42


def wrong_order_returns_before_parameters():  # NPD007
    """Calculate something.

    Returns
    -------
    int
        Output value.

    Parameters
    ----------
    x : int
        Input value.
    """
    return 42


def wrong_order_examples_before_returns():  # NPD007
    """Calculate something.

    Parameters
    ----------
    x : int
        Input value.

    Examples
    --------
    >>> wrong_order_examples_before_returns()
    42

    Returns
    -------
    int
        Output value.
    """
    return 42


def wrong_order_notes_before_see_also():  # NPD007
    """Calculate something.

    Parameters
    ----------
    x : int
        Input value.

    Notes
    -----
    Some notes here.

    See Also
    --------
    other_function : Related function.

    Returns
    -------
    int
        Output value.
    """
    return 42


def correct_order_with_raises():
    """Calculate something.

    Parameters
    ----------
    x : int
        Input value.

    Returns
    -------
    int
        Output value.

    Raises
    ------
    ValueError
        If x is negative.

    See Also
    --------
    other_function : Related function.

    Notes
    -----
    Some notes here.

    Examples
    --------
    >>> correct_order_with_raises()
    42
    """
    return 42


def correct_order_minimal():
    """Calculate something.

    Returns
    -------
    int
        Output value.
    """
    return 42


def wrong_order_see_also_before_raises():  # NPD007
    """Calculate something.

    Parameters
    ----------
    x : int
        Input value.

    Returns
    -------
    int
        Output value.

    See Also
    --------
    other_function : Related function.

    Raises
    ------
    ValueError
        If x is negative.
    """
    return 42


def wrong_order_multiple():  # NPD007
    """Calculate something.

    Raises
    ------
    ValueError
        If x is negative.

    See Also
    --------
    other_function : Related function.

    Returns
    -------
    int
        Output value.

    Parameters
    ----------
    x : int
        Input value.

    """
    return 42


def google_style_wrong_order():
    """Calculate something.

    Returns:
        int: Output value.

    Args:
        x (int): Input value.
    """
    return 42


def single_section_only():
    """Calculate something.

    Returns
    -------
    int
        Output value.
    """
    return 42


def no_sections():
    """Just a simple docstring with no sections at all."""
    return 42


def complex_content_wrong_order():  # NPD007
    """Calculate something complex.

    Examples
    --------
    Here's how to use it:

    >>> result = complex_content_wrong_order(5)
    >>> print(result)
    10

    You can also do:
    >>> complex_content_wrong_order(-1)
    0

    Parameters
    ----------
    x : int
        The input value.
        Can be positive or negative.
        Multiple lines of description here.

    Returns
    -------
    int
        The output value.
        This is also multi-line.
    """
    return max(0, x * 2)


def extra_text_between_sections():  # NPD007
    """Calculate something.

    Returns
    -------
    int
        Output value.

    This is some random text that appears between sections.
    It's not part of any official section.

    Parameters
    ----------
    x : int
        Input value.
    """
    return 42
