def func():
    import numpy as np

    np.add_docstring

    np.add_newdoc

    np.add_newdoc_ufunc

    np.asfarray([1,2,3])

    np.byte_bounds(np.array([1,2,3]))

    np.cast

    np.cfloat(12+34j)

    np.clongfloat(12+34j)

    np.compat

    np.complex_(12+34j)

    np.DataSource

    np.deprecate

    np.deprecate_with_doc

    np.disp(10)

    np.fastCopyAndTranspose

    np.find_common_type

    np.get_array_wrap

    np.float_

    np.geterrobj

    np.Inf

    np.Infinity

    np.infty

    np.issctype

    np.issubclass_(np.int32, np.integer)

    np.issubsctype

    np.mat

    np.maximum_sctype

    np.NaN

    np.nbytes[np.int64]

    np.NINF

    np.NZERO

    np.longcomplex(12+34j)

    np.longfloat(12+34j)

    np.lookfor

    np.NAN

    try:
        from numpy.lib.npyio import DataSource
    except ImportError:
        from numpy import DataSource

    DataSource("foo").abspath()  # fine (`except ImportError` branch)

    try:
        from numpy.rec import format_parser
        from numpy import clongdouble
    except ModuleNotFoundError:
        from numpy import format_parser
        from numpy import longcomplex as clongdouble

    format_parser("foo")  # fine (`except ModuleNotFoundError` branch)
    clongdouble(42)  # fine (`except ModuleNotFoundError` branch)
