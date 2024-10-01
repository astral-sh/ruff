from typing import Any
from typing_extensions import TypeAlias
from weakref import ReferenceType

__all__ = ["local"]
_LocalDict: TypeAlias = dict[Any, Any]

class _localimpl:
    key: str
    dicts: dict[int, tuple[ReferenceType[Any], _LocalDict]]
    def get_dict(self) -> _LocalDict: ...
    def create_dict(self) -> _LocalDict: ...

class local:
    def __getattribute__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: Any) -> None: ...
    def __delattr__(self, name: str) -> None: ...
