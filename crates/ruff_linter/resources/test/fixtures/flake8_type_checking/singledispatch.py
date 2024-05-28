"""Test module."""
from __future__ import annotations

from functools import singledispatch
from pathlib import Path
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


@singledispatch
def process_path(a: int | str, p: Path) -> int:
    """Convert arg to array or leaves it as sparse matrix."""
    msg = f"Unhandled type {type(a)}"
    raise NotImplementedError(msg)


@process_path.register
def _(a: int, p: Path) -> int:
    return asarray(a)


@process_path.register
def _(a: str, p: Path) -> int:
    return a


def _(a: DataFrame, p: Path) -> DataFrame:
    return a
