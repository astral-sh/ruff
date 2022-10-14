"""This is the docstring for the example.py module.  Modules names should
have short, all-lowercase names.  The module name may have underscores if
this improves readability.

Every module should have a docstring at the very top of the file.  The
module's docstring may extend over multiple lines.  If your docstring does
extend over multiple lines, the closing three quotation marks must be on
a line by itself, preferably preceded by a blank line.

"""

# Example source file from the official "numpydoc docstring guide"
# documentation (with the modification of commenting out all the original
# ``import`` lines, plus adding this note and ``Expectation`` code):
#     * As HTML: https://numpydoc.readthedocs.io/en/latest/example.html
#     * Source Python:
#         https://github.com/numpy/numpydoc/blob/master/doc/example.py

# from __future__ import division, absolute_import, print_function
#
# import os  # standard library imports first
#
# Do NOT import using *, e.g. from numpy import *
#
# Import the module using
#
#   import numpy
#
# instead or import individual functions as needed, e.g
#
#  from numpy import array, zeros
#
# If you prefer the use of abbreviated module names, we suggest the
# convention used by NumPy itself::
#
# import numpy as np
# import matplotlib as mpl
# import matplotlib.pyplot as plt
#
# These abbreviated names are not to be used in docstrings; users must
# be able to paste and execute docstrings after importing only the
# numpy module itself, unabbreviated.

import os
from .expected import Expectation

expectation = Expectation()
expect = expectation.expect

# module docstring expected violations:
expectation.expected.add((
    os.path.normcase(__file__),
    "D205: 1 blank line required between summary line and description "
    "(found 0)"))
expectation.expected.add((
    os.path.normcase(__file__),
    "D213: Multi-line docstring summary should start at the second line"))
expectation.expected.add((
    os.path.normcase(__file__),
    "D400: First line should end with a period (not 'd')"))
expectation.expected.add((
    os.path.normcase(__file__),
    "D404: First word of the docstring should not be `This`"))
expectation.expected.add((
    os.path.normcase(__file__),
    "D415: First line should end with a period, question mark, or exclamation "
    "point (not 'd')"))


@expect("D213: Multi-line docstring summary should start at the second line",
        arg_count=3)
@expect("D401: First line should be in imperative mood; try rephrasing "
        "(found 'A')", arg_count=3)
@expect("D413: Missing blank line after last section ('Examples')",
        arg_count=3)
def foo(var1, var2, long_var_name='hi'):
    r"""A one-line summary that does not use variable names.

    Several sentences providing an extended description. Refer to
    variables using back-ticks, e.g. `var`.

    Parameters
    ----------
    var1 : array_like
        Array_like means all those objects -- lists, nested lists, etc. --
        that can be converted to an array.  We can also refer to
        variables like `var1`.
    var2 : int
        The type above can either refer to an actual Python type
        (e.g. ``int``), or describe the type of the variable in more
        detail, e.g. ``(N,) ndarray`` or ``array_like``.
    long_var_name : {'hi', 'ho'}, optional
        Choices in brackets, default first when optional.

    Returns
    -------
    type
        Explanation of anonymous return value of type ``type``.
    describe : type
        Explanation of return value named `describe`.
    out : type
        Explanation of `out`.
    type_without_description

    Other Parameters
    ----------------
    only_seldom_used_keywords : type
        Explanation
    common_parameters_listed_above : type
        Explanation

    Raises
    ------
    BadException
        Because you shouldn't have done that.

    See Also
    --------
    numpy.array : Relationship (optional).
    numpy.ndarray : Relationship (optional), which could be fairly long, in
                    which case the line wraps here.
    numpy.dot, numpy.linalg.norm, numpy.eye

    Notes
    -----
    Notes about the implementation algorithm (if needed).

    This can have multiple paragraphs.

    You may include some math:

    .. math:: X(e^{j\omega } ) = x(n)e^{ - j\omega n}

    And even use a Greek symbol like :math:`\omega` inline.

    References
    ----------
    Cite the relevant literature, e.g. [1]_.  You may also cite these
    references in the notes section above.

    .. [1] O. McNoleg, "The integration of GIS, remote sensing,
       expert systems and adaptive co-kriging for environmental habitat
       modelling of the Highland Haggis using object-oriented, fuzzy-logic
       and neural-network techniques," Computers & Geosciences, vol. 22,
       pp. 585-588, 1996.

    Examples
    --------
    These are written in doctest format, and should illustrate how to
    use the function.

    >>> a = [1, 2, 3]
    >>> print([x + 3 for x in a])
    [4, 5, 6]
    >>> print("a\nb")
    a
    b
    """
    # After closing class docstring, there should be one blank line to
    # separate following codes (according to PEP257).
    # But for function, method and module, there should be no blank lines
    # after closing the docstring.
    pass
