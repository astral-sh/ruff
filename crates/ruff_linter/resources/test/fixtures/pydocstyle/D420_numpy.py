"""Tests for D420 (section order) with NumPy convention."""


def correct_order():
    """Summary.

    Extended Summary
    ----------------
    More details.

    Parameters
    ----------
    x : int
        Description.

    Returns
    -------
    int

    Yields
    ------
    int

    Receives
    --------
    int

    Other Parameters
    ----------------
    y : int
        Description.

    Raises
    ------
    ValueError

    Warns
    -----
    UserWarning

    Warnings
    --------
    Be careful.

    See Also
    --------
    other_func

    Notes
    -----
    Some notes.

    References
    ----------
    [1] Reference.

    Examples
    --------
    >>> correct_order()

    Attributes
    ----------
    attr : int

    Methods
    -------
    method
    """


def single_swap():
    """Summary.

    Notes
    -----
    Some notes.

    Returns
    -------
    int
    """


def multiple_out_of_order():
    """Summary.

    Examples
    --------
    >>> func()

    Returns
    -------
    int

    Notes
    -----
    Some notes.
    """


def single_section():
    """Summary.

    Returns
    -------
    int
    """


def notes_before_warns():
    """Summary.

    Notes
    -----
    Some notes.

    Warns
    -----
    UserWarning
    """


def other_params_alias():
    """Summary.

    Other Params
    ------------
    y : int
        Description.

    Parameters
    ----------
    x : int
        Description.
    """


def correct_partial():
    """Summary.

    Parameters
    ----------
    x : int
        Description.

    Returns
    -------
    int

    Examples
    --------
    >>> func()
    """


class CorrectClassDocstring:
    """Summary.

    Attributes
    ----------
    x : int
        Description.

    Methods
    -------
    do_something
    """


class WrongClassDocstring:
    """Summary.

    Methods
    -------
    do_something

    Attributes
    ----------
    x : int
        Description.
    """


def unrecognized_sections_only():
    """Summary.

    Custom Section
    --------------
    Some content.

    Another Custom
    --------------
    More content.
    """


def unrecognized_mixed_with_recognized():
    """Summary.

    Parameters
    ----------
    x : int
        Description.

    Custom Section
    --------------
    Some content.

    Returns
    -------
    int
    """
