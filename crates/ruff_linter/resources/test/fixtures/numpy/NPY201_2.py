def func():
    import numpy as np

    np.obj2sctype(int)

    np.PINF

    np.PZERO

    np.recfromcsv

    np.recfromtxt

    np.round_(12.34)

    np.safe_eval

    np.sctype2char

    np.sctypes

    np.seterrobj

    np.set_numeric_ops

    np.set_string_function

    np.singlecomplex(12+1j)

    np.string_("asdf")

    np.source

    np.tracemalloc_domain

    np.unicode_("asf")

    np.who()

    np.row_stack(([1,2], [3,4]))

    np.alltrue([True, True])

    np.sometrue([True, False])

    np.cumproduct([1, 2, 3])

    np.product([1, 2, 3])

    np.trapz([1, 2, 3])

    np.in1d([1, 2], [1, 3, 5])

    np.AxisError

    np.ComplexWarning

    np.compare_chararrays

    try:
        np.all([True, True])
    except TypeError:
        np.alltrue([True, True])  # Should emit a warning here (`except TypeError`, not `except AttributeError`)

    try:
        np.anyyyy([True, True])
    except AttributeError:
        np.sometrue([True, True])  # Should emit a warning here
                                   # (must have an attribute access of the undeprecated name in the `try` body for it to be ignored)

    try:
        exc = np.exceptions.ComplexWarning
    except AttributeError:
        exc = np.ComplexWarning   # `except AttributeError` means that this is okay

    raise exc
