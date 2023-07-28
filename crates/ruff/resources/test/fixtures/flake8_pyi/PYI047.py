import typing
import sys
from typing import TypeAlias


_UnusedPrivateTypeAlias: TypeAlias = int | None
_T: typing.TypeAlias = str

# OK
_UsedPrivateTypeAlias: TypeAlias = int | None

def func(arg: _UsedPrivateTypeAlias) -> _UsedPrivateTypeAlias:
  ...


if sys.version_info > (3, 9):
  _PrivateTypeAlias: TypeAlias = str | None
else:
  _PrivateTypeAlias: TypeAlias = float | None


def func2(arg: _PrivateTypeAlias) -> None: ...
