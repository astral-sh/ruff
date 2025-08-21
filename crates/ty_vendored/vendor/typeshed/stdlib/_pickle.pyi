from _typeshed import ReadableBuffer, SupportsWrite
from collections.abc import Callable, Iterable, Iterator, Mapping
from pickle import PickleBuffer as PickleBuffer
from typing import Any, Protocol, type_check_only
from typing_extensions import TypeAlias

@type_check_only
class _ReadableFileobj(Protocol):
    def read(self, n: int, /) -> bytes: ...
    def readline(self) -> bytes: ...

_BufferCallback: TypeAlias = Callable[[PickleBuffer], Any] | None

_ReducedType: TypeAlias = (
    str
    | tuple[Callable[..., Any], tuple[Any, ...]]
    | tuple[Callable[..., Any], tuple[Any, ...], Any]
    | tuple[Callable[..., Any], tuple[Any, ...], Any, Iterator[Any] | None]
    | tuple[Callable[..., Any], tuple[Any, ...], Any, Iterator[Any] | None, Iterator[Any] | None]
)

def dump(
    obj: Any,
    file: SupportsWrite[bytes],
    protocol: int | None = None,
    *,
    fix_imports: bool = True,
    buffer_callback: _BufferCallback = None,
) -> None: ...
def dumps(
    obj: Any, protocol: int | None = None, *, fix_imports: bool = True, buffer_callback: _BufferCallback = None
) -> bytes: ...
def load(
    file: _ReadableFileobj,
    *,
    fix_imports: bool = True,
    encoding: str = "ASCII",
    errors: str = "strict",
    buffers: Iterable[Any] | None = (),
) -> Any: ...
def loads(
    data: ReadableBuffer,
    /,
    *,
    fix_imports: bool = True,
    encoding: str = "ASCII",
    errors: str = "strict",
    buffers: Iterable[Any] | None = (),
) -> Any: ...

class PickleError(Exception): ...
class PicklingError(PickleError): ...
class UnpicklingError(PickleError): ...

@type_check_only
class PicklerMemoProxy:
    def clear(self, /) -> None: ...
    def copy(self, /) -> dict[int, tuple[int, Any]]: ...

class Pickler:
    fast: bool
    dispatch_table: Mapping[type, Callable[[Any], _ReducedType]]
    reducer_override: Callable[[Any], Any]
    bin: bool  # undocumented
    def __init__(
        self,
        file: SupportsWrite[bytes],
        protocol: int | None = None,
        fix_imports: bool = True,
        buffer_callback: _BufferCallback = None,
    ) -> None: ...
    @property
    def memo(self) -> PicklerMemoProxy: ...
    @memo.setter
    def memo(self, value: PicklerMemoProxy | dict[int, tuple[int, Any]]) -> None: ...
    def dump(self, obj: Any, /) -> None: ...
    def clear_memo(self) -> None: ...

    # this method has no default implementation for Python < 3.13
    def persistent_id(self, obj: Any, /) -> Any: ...

@type_check_only
class UnpicklerMemoProxy:
    def clear(self, /) -> None: ...
    def copy(self, /) -> dict[int, tuple[int, Any]]: ...

class Unpickler:
    def __init__(
        self,
        file: _ReadableFileobj,
        *,
        fix_imports: bool = True,
        encoding: str = "ASCII",
        errors: str = "strict",
        buffers: Iterable[Any] | None = (),
    ) -> None: ...
    @property
    def memo(self) -> UnpicklerMemoProxy: ...
    @memo.setter
    def memo(self, value: UnpicklerMemoProxy | dict[int, tuple[int, Any]]) -> None: ...
    def load(self) -> Any: ...
    def find_class(self, module_name: str, global_name: str, /) -> Any: ...

    # this method has no default implementation for Python < 3.13
    def persistent_load(self, pid: Any, /) -> Any: ...
