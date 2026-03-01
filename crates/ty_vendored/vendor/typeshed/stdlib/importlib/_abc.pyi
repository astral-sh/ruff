"""Subset of importlib.abc used to reduce importlib.util imports."""

import sys
import types
from abc import ABCMeta
from importlib.machinery import ModuleSpec
from typing_extensions import deprecated

if sys.version_info >= (3, 10):
    class Loader(metaclass=ABCMeta):
        """Abstract base class for import loaders."""

        def load_module(self, fullname: str) -> types.ModuleType:
            """Return the loaded module.

            The module must be added to sys.modules and have import-related
            attributes set properly.  The fullname is a str.

            ImportError is raised on failure.

            This method is deprecated in favor of loader.exec_module(). If
            exec_module() exists then it is used to provide a backwards-compatible
            functionality for this method.

            """
        if sys.version_info < (3, 12):
            @deprecated(
                "Deprecated since Python 3.4; removed in Python 3.12. "
                "The module spec is now used by the import machinery to generate a module repr."
            )
            def module_repr(self, module: types.ModuleType) -> str:
                """Return a module's repr.

                Used by the module type when the method does not raise
                NotImplementedError.

                This method is deprecated.

                """

        def create_module(self, spec: ModuleSpec) -> types.ModuleType | None:
            """Return a module to initialize and into which to load.

            This method should raise ImportError if anything prevents it
            from creating a new module.  It may return None to indicate
            that the spec should create the new module.
            """
        # Not defined on the actual class for backwards-compatibility reasons,
        # but expected in new code.
        def exec_module(self, module: types.ModuleType) -> None: ...
