import sys
from importlib._bootstrap import BuiltinImporter as BuiltinImporter, FrozenImporter as FrozenImporter, ModuleSpec as ModuleSpec
from importlib._bootstrap_external import (
    BYTECODE_SUFFIXES as BYTECODE_SUFFIXES,
    DEBUG_BYTECODE_SUFFIXES as DEBUG_BYTECODE_SUFFIXES,
    EXTENSION_SUFFIXES as EXTENSION_SUFFIXES,
    OPTIMIZED_BYTECODE_SUFFIXES as OPTIMIZED_BYTECODE_SUFFIXES,
    SOURCE_SUFFIXES as SOURCE_SUFFIXES,
    ExtensionFileLoader as ExtensionFileLoader,
    FileFinder as FileFinder,
    PathFinder as PathFinder,
    SourceFileLoader as SourceFileLoader,
    SourcelessFileLoader as SourcelessFileLoader,
    WindowsRegistryFinder as WindowsRegistryFinder,
)

if sys.version_info >= (3, 11):
    from importlib._bootstrap_external import NamespaceLoader as NamespaceLoader
if sys.version_info >= (3, 14):
    from importlib._bootstrap_external import AppleFrameworkLoader as AppleFrameworkLoader

def all_suffixes() -> list[str]: ...

if sys.version_info >= (3, 14):
    __all__ = [
        "AppleFrameworkLoader",
        "BYTECODE_SUFFIXES",
        "BuiltinImporter",
        "DEBUG_BYTECODE_SUFFIXES",
        "EXTENSION_SUFFIXES",
        "ExtensionFileLoader",
        "FileFinder",
        "FrozenImporter",
        "ModuleSpec",
        "NamespaceLoader",
        "OPTIMIZED_BYTECODE_SUFFIXES",
        "PathFinder",
        "SOURCE_SUFFIXES",
        "SourceFileLoader",
        "SourcelessFileLoader",
        "WindowsRegistryFinder",
        "all_suffixes",
    ]
