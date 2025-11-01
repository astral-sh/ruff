import pandas as pd
import numpy as np


def test_numpy_unique_inverse():
    unique = np.unique_inverse([1, 2, 3, 2, 1])
    result = unique.values


def test_numpy_unique_all():
    unique = np.unique_all([1, 2, 3, 2, 1])
    result = unique.values


def test_numpy_unique_counts():
    unique = np.unique_counts([1, 2, 3, 2, 1])
    result = unique.values


def test_numpy_typed_unique_inverse():
    from typing import TYPE_CHECKING
    if TYPE_CHECKING:
        from numpy.lib._arraysetops_impl import UniqueInverseResult
    unique: UniqueInverseResult[np.uint64] = np.unique_inverse([1, 2, 3, 2, 1])
    result = unique.values


def test_simple_non_pandas():
    p = 1
    result = p.values


def test_pandas_dataframe_values():
    """This should trigger PD011 - pandas DataFrame .values usage"""
    import pandas as pd
    x = pd.DataFrame()
    result = x.values

