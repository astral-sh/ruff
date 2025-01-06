from threading import RLock
from typing import Any
from typing_extensions import Self, TypeAlias
from weakref import ReferenceType

__all__ = ["local"]
_LocalDict: TypeAlias = dict[Any, Any]

class _localimpl:
    key: str
    dicts: dict[int, tuple[ReferenceType[Any], _LocalDict]]
    # Keep localargs in sync with the *args, **kwargs annotation on local.__new__
    localargs: tuple[list[Any], dict[str, Any]]
    locallock: RLock
    def get_dict(self) -> _LocalDict: ...
    def create_dict(self) -> _LocalDict: ...

class local:
    def __new__(cls, /, *args: Any, **kw: Any) -> Self: ...
    def __getattribute__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: Any) -> None: ...
    def __delattr__(self, name: str) -> None: ...
