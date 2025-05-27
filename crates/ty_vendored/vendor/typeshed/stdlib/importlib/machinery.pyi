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

def all_suffixes() -> list[str]: ...
