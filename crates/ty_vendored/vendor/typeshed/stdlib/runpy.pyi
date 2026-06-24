"""runpy.py - locating and running Python code using the module namespace

Provides support for locating and running Python scripts using the Python
module namespace instead of the native filesystem.

This allows Python code to play nicely with non-filesystem based PEP 302
importers when locating support scripts as well as when importing modules.
"""

from _typeshed import Unused
from types import ModuleType
from typing import Any
from typing_extensions import Self

__all__ = ["run_module", "run_path"]

class _TempModule:
    """Temporarily replace a module in sys.modules with an empty namespace"""

    mod_name: str
    module: ModuleType
    def __init__(self, mod_name: str) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...

class _ModifiedArgv0:
    value: Any
    def __init__(self, value: Any) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(self, *args: Unused) -> None: ...

def run_module(
    mod_name: str, init_globals: dict[str, Any] | None = None, run_name: str | None = None, alter_sys: bool = False
) -> dict[str, Any]:
    """Execute a module's code without importing it.

    mod_name -- an absolute module name or package name.

    Optional arguments:
    init_globals -- dictionary used to pre-populate the module’s
    globals dictionary before the code is executed.

    run_name -- if not None, this will be used for setting __name__;
    otherwise, __name__ will be set to mod_name + '__main__' if the
    named module is a package and to just mod_name otherwise.

    alter_sys -- if True, sys.argv[0] is updated with the value of
    __file__ and sys.modules[__name__] is updated with a temporary
    module object for the module being executed. Both are
    restored to their original values before the function returns.

    Returns the resulting module globals dictionary.
    """

def run_path(path_name: str, init_globals: dict[str, Any] | None = None, run_name: str | None = None) -> dict[str, Any]:
    """Execute code located at the specified filesystem location.

    path_name -- filesystem location of a Python script, zipfile,
    or directory containing a top level __main__.py script.

    Optional arguments:
    init_globals -- dictionary used to pre-populate the module’s
    globals dictionary before the code is executed.

    run_name -- if not None, this will be used to set __name__;
    otherwise, '<run_path>' will be used for __name__.

    Returns the resulting module globals dictionary.
    """
