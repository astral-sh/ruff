import sys
from _typeshed import StrOrBytesPath, SupportsRead
from _typeshed.importlib import LoaderProtocol, MetaPathFinderProtocol, PathEntryFinderProtocol
from collections.abc import Callable, Iterable, Iterator
from typing import IO, Any, NamedTuple, TypeVar
from typing_extensions import deprecated

__all__ = [
    "get_importer",
    "iter_importers",
    "walk_packages",
    "iter_modules",
    "get_data",
    "read_code",
    "extend_path",
    "ModuleInfo",
]
if sys.version_info < (3, 14):
    __all__ += ["get_loader", "find_loader"]
if sys.version_info < (3, 12):
    __all__ += ["ImpImporter", "ImpLoader"]

_PathT = TypeVar("_PathT", bound=Iterable[str])

class ModuleInfo(NamedTuple):
    module_finder: MetaPathFinderProtocol | PathEntryFinderProtocol
    name: str
    ispkg: bool

def extend_path(path: _PathT, name: str) -> _PathT: ...

if sys.version_info < (3, 12):
    @deprecated("Deprecated since Python 3.3; removed in Python 3.12. Use the `importlib` module instead.")
    class ImpImporter:
        def __init__(self, path: StrOrBytesPath | None = None) -> None: ...

    @deprecated("Deprecated since Python 3.3; removed in Python 3.12. Use the `importlib` module instead.")
    class ImpLoader:
        def __init__(self, fullname: str, file: IO[str], filename: StrOrBytesPath, etc: tuple[str, str, int]) -> None: ...

if sys.version_info < (3, 14):
    if sys.version_info >= (3, 12):
        @deprecated("Deprecated since Python 3.12; removed in Python 3.14. Use `importlib.util.find_spec()` instead.")
        def find_loader(fullname: str) -> LoaderProtocol | None: ...
        @deprecated("Deprecated since Python 3.12; removed in Python 3.14. Use `importlib.util.find_spec()` instead.")
        def get_loader(module_or_name: str) -> LoaderProtocol | None: ...
    else:
        def find_loader(fullname: str) -> LoaderProtocol | None: ...
        def get_loader(module_or_name: str) -> LoaderProtocol | None: ...

def get_importer(path_item: StrOrBytesPath) -> PathEntryFinderProtocol | None: ...
def iter_importers(fullname: str = "") -> Iterator[MetaPathFinderProtocol | PathEntryFinderProtocol]: ...
def iter_modules(path: Iterable[StrOrBytesPath] | None = None, prefix: str = "") -> Iterator[ModuleInfo]: ...
def read_code(stream: SupportsRead[bytes]) -> Any: ...  # undocumented
def walk_packages(
    path: Iterable[StrOrBytesPath] | None = None, prefix: str = "", onerror: Callable[[str], object] | None = None
) -> Iterator[ModuleInfo]: ...
def get_data(package: str, resource: str) -> bytes | None: ...
def resolve_name(name: str) -> Any: ...
