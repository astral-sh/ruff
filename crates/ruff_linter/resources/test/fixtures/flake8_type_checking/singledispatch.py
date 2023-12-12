"""Test module."""
from __future__ import annotations

from functools import singledispatch
from typing import TYPE_CHECKING

from numpy import asarray
from numpy.typing import ArrayLike
from scipy.sparse import spmatrix
from pandas import DataFrame

if TYPE_CHECKING:
    from numpy import ndarray


@singledispatch
def to_array_or_mat(a: ArrayLike | spmatrix) -> ndarray | spmatrix:
    """Convert arg to array or leaves it as sparse matrix."""
    msg = f"Unhandled type {type(a)}"
    raise NotImplementedError(msg)


@to_array_or_mat.register
def _(a: ArrayLike) -> ndarray:
    return asarray(a)


@to_array_or_mat.register
def _(a: spmatrix) -> spmatrix:
    return a


def _(a: DataFrame) -> DataFrame:
    return a
