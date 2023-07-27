import typing
from typing import TypeAlias


_UnusedPrivateTypeAlias: TypeAlias = int | None
_T: typing.TypeAlias = str

# OK
_UsedPrivateTypeAlias: TypeAlias = int | None

def func(arg: _UsedPrivateTypeAlias) -> _UsedPrivateTypeAlias:
  ...
