import sys

# Even though this file is 3.11+ only, Pyright will complain in stubtest for older versions.
if sys.version_info >= (3, 11):
    import types
    from collections.abc import Callable
    from contextlib import AbstractContextManager
    from importlib.resources.abc import ResourceReader, Traversable
    from pathlib import Path
    from typing import Literal, overload
    from typing_extensions import TypeAlias, deprecated

    Package: TypeAlias = str | types.ModuleType

    if sys.version_info >= (3, 12):
        Anchor: TypeAlias = Package

        def package_to_anchor(
            func: Callable[[Anchor | None], Traversable],
        ) -> Callable[[Anchor | None, Anchor | None], Traversable]:
            """
            Replace 'package' parameter as 'anchor' and warn about the change.

            Other errors should fall through.

            >>> files('a', 'b')
            Traceback (most recent call last):
            TypeError: files() takes from 0 to 1 positional arguments but 2 were given

            Remove this compatibility in Python 3.14.
            """

        @overload
        def files(anchor: Anchor | None = None) -> Traversable:
            """
            Get a Traversable resource for an anchor.
            """

        @overload
        @deprecated("Deprecated since Python 3.12; will be removed in Python 3.15. Use `anchor` parameter instead.")
        def files(package: Anchor | None = None) -> Traversable: ...

    else:
        def files(package: Package) -> Traversable:
            """
            Get a Traversable resource from a package
            """

    def get_resource_reader(package: types.ModuleType) -> ResourceReader | None:
        """
        Return the package's loader if it's a ResourceReader.
        """
    if sys.version_info >= (3, 12):
        def resolve(cand: Anchor | None) -> types.ModuleType: ...

    else:
        def resolve(cand: Package) -> types.ModuleType: ...

    if sys.version_info < (3, 12):
        def get_package(package: Package) -> types.ModuleType:
            """Take a package name or module object and return the module.

            Raise an exception if the resolved module is not a package.
            """

    def from_package(package: types.ModuleType) -> Traversable:
        """
        Return a Traversable object for the given package.

        """

    def as_file(path: Traversable) -> AbstractContextManager[Path, Literal[False]]:
        """
        Given a Traversable object, return that object as a
        path on the local file system in a context manager.
        """
