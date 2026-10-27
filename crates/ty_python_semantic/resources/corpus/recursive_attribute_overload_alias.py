# Regression for cyclic attribute inference through an aliased overload return.

from typing import Any, Generic, TypeVar, overload

_T = TypeVar("_T", covariant=True)
type Dynamic = Any


class ndarray(Generic[_T]):
    def __getitem__(self, key):
        pass


@overload
def asarray(a) -> ndarray[Any | Dynamic]: ...
@overload
def asarray(a: ndarray[_T]) -> ndarray[_T]: ...
def asarray(a): ...


class _coo_base:
    def __init__(self):
        self.coords = (ndarray(),)

    def row(self, new_row):
        self.coords = (*self.coords, asarray(new_row)) + self.coords
        self.coords = (asarray(self.coords),)

    def col(self, new_col):
        self.coords = self.coords + (asarray(new_col),)
        self.coords = (*(idx[0] for idx in self.coords),)
