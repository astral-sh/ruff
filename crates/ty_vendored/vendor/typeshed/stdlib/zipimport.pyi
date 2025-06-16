import sys
from _typeshed import StrOrBytesPath
from importlib.machinery import ModuleSpec
from types import CodeType, ModuleType
from typing_extensions import deprecated

if sys.version_info >= (3, 10):
    from importlib.readers import ZipReader
else:
    from importlib.abc import ResourceReader

if sys.version_info >= (3, 10):
    from _frozen_importlib_external import _LoaderBasics
else:
    _LoaderBasics = object

__all__ = ["ZipImportError", "zipimporter"]

class ZipImportError(ImportError): ...

class zipimporter(_LoaderBasics):
    archive: str
    prefix: str
    if sys.version_info >= (3, 11):
        def __init__(self, path: str) -> None: ...
    else:
        def __init__(self, path: StrOrBytesPath) -> None: ...

    if sys.version_info < (3, 12):
        def find_loader(self, fullname: str, path: str | None = None) -> tuple[zipimporter | None, list[str]]: ...  # undocumented
        def find_module(self, fullname: str, path: str | None = None) -> zipimporter | None: ...

    def get_code(self, fullname: str) -> CodeType: ...
    def get_data(self, pathname: str) -> bytes: ...
    def get_filename(self, fullname: str) -> str: ...
    if sys.version_info >= (3, 14):
        def get_resource_reader(self, fullname: str) -> ZipReader: ...  # undocumented
    elif sys.version_info >= (3, 10):
        def get_resource_reader(self, fullname: str) -> ZipReader | None: ...  # undocumented
    else:
        def get_resource_reader(self, fullname: str) -> ResourceReader | None: ...  # undocumented

    def get_source(self, fullname: str) -> str | None: ...
    def is_package(self, fullname: str) -> bool: ...
    @deprecated("Deprecated since 3.10; use exec_module() instead")
    def load_module(self, fullname: str) -> ModuleType: ...
    if sys.version_info >= (3, 10):
        def exec_module(self, module: ModuleType) -> None: ...
        def create_module(self, spec: ModuleSpec) -> None: ...
        def find_spec(self, fullname: str, target: ModuleType | None = None) -> ModuleSpec | None: ...
        def invalidate_caches(self) -> None: ...
