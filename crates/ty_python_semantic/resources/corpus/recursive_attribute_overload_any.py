# Regression for a convergence panic in SciPy's COO coordinate updates.

from typing import Any, Generic, TypeVar, overload
_T = TypeVar('_T', covariant=True)

class ndarray(Generic[_T]):

    def __getitem__(self, key):
        pass

@overload
def asarray(a: ndarray[_T]) -> ndarray[_T]:
    pass

@overload
def asarray(a) -> ndarray[Any]:
    pass

def asarray(a):
    pass

class _coo_base:

    def __init__(self):
        self.coords = (ndarray(),)

    def row(self, new_row):
        self.coords = (*self.coords, asarray(new_row)) + self.coords
        self.coords = (asarray(self.coords),)

    def col(self, new_col):
        self.coords = self.coords + (asarray(new_col),)
        self.coords = (*(idx[0] for idx in self.coords),)
