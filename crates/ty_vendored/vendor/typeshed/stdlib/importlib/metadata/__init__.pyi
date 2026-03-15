import abc
import pathlib
import sys
import types
from _collections_abc import dict_keys, dict_values
from _typeshed import StrPath
from collections.abc import Iterable, Iterator, Mapping
from email.message import Message
from importlib.abc import MetaPathFinder
from os import PathLike
from pathlib import Path
from re import Pattern
from typing import Any, ClassVar, Generic, NamedTuple, TypeVar, overload
from typing_extensions import Self, TypeAlias, deprecated, disjoint_base

_T = TypeVar("_T")
_KT = TypeVar("_KT")
_VT = TypeVar("_VT")

__all__ = [
    "Distribution",
    "DistributionFinder",
    "PackageNotFoundError",
    "distribution",
    "distributions",
    "entry_points",
    "files",
    "metadata",
    "requires",
    "version",
]

if sys.version_info >= (3, 10):
    __all__ += ["PackageMetadata", "packages_distributions"]

if sys.version_info >= (3, 10):
    from importlib.metadata._meta import PackageMetadata as PackageMetadata, SimplePath
    def packages_distributions() -> Mapping[str, list[str]]:
        """
        Return a mapping of top-level packages to their
        distributions.

        >>> import collections.abc
        >>> pkgs = packages_distributions()
        >>> all(isinstance(dist, collections.abc.Sequence) for dist in pkgs.values())
        True
        """
    _SimplePath: TypeAlias = SimplePath

else:
    _SimplePath: TypeAlias = Path

class PackageNotFoundError(ModuleNotFoundError):
    """The package was not found."""

    @property
    def name(self) -> str:  # type: ignore[override]
        """module name"""

if sys.version_info >= (3, 13):
    _EntryPointBase = object
elif sys.version_info >= (3, 11):
    class DeprecatedTuple:
        """
        Provide subscript item access for backward compatibility.

        >>> recwarn = getfixture('recwarn')
        >>> ep = EntryPoint(name='name', value='value', group='group')
        >>> ep[:]
        ('name', 'value', 'group')
        >>> ep[0]
        'name'
        >>> len(recwarn)
        1
        """

        def __getitem__(self, item: int) -> str: ...

    _EntryPointBase = DeprecatedTuple
else:
    class _EntryPointBase(NamedTuple):
        name: str
        value: str
        group: str

if sys.version_info >= (3, 11):
    class EntryPoint(_EntryPointBase):
        """An entry point as defined by Python packaging conventions.

        See `the packaging docs on entry points
        <https://packaging.python.org/specifications/entry-points/>`_
        for more information.

        >>> ep = EntryPoint(
        ...     name=None, group=None, value='package.module:attr [extra1, extra2]')
        >>> ep.module
        'package.module'
        >>> ep.attr
        'attr'
        >>> ep.extras
        ['extra1', 'extra2']
        """

        pattern: ClassVar[Pattern[str]]
        name: str
        value: str
        group: str

        def __init__(self, name: str, value: str, group: str) -> None: ...
        def load(self) -> Any:  # Callable[[], Any] or an importable module
            """Load the entry point from its definition. If only a module
            is indicated by the value, return that module. Otherwise,
            return the named object.
            """

        @property
        def extras(self) -> list[str]: ...
        @property
        def module(self) -> str: ...
        @property
        def attr(self) -> str: ...
        dist: ClassVar[Distribution | None]
        def matches(
            self,
            *,
            name: str = ...,
            value: str = ...,
            group: str = ...,
            module: str = ...,
            attr: str = ...,
            extras: list[str] = ...,
        ) -> bool:  # undocumented
            """
            EntryPoint matches the given parameters.

            >>> ep = EntryPoint(group='foo', name='bar', value='bing:bong [extra1, extra2]')
            >>> ep.matches(group='foo')
            True
            >>> ep.matches(name='bar', value='bing:bong [extra1, extra2]')
            True
            >>> ep.matches(group='foo', name='other')
            False
            >>> ep.matches()
            True
            >>> ep.matches(extras=['extra1', 'extra2'])
            True
            >>> ep.matches(module='bing')
            True
            >>> ep.matches(attr='bong')
            True
            """

        def __hash__(self) -> int: ...
        def __eq__(self, other: object) -> bool: ...
        def __lt__(self, other: object) -> bool: ...
        if sys.version_info < (3, 12):
            def __iter__(self) -> Iterator[Any]:  # result of iter((str, Self)), really
                """
                Supply iter so one may construct dicts of EntryPoints by name.
                """

else:
    @disjoint_base
    class EntryPoint(_EntryPointBase):
        """An entry point as defined by Python packaging conventions.

        See `the packaging docs on entry points
        <https://packaging.python.org/specifications/entry-points/>`_
        for more information.

        >>> ep = EntryPoint(
        ...     name=None, group=None, value='package.module:attr [extra1, extra2]')
        >>> ep.module
        'package.module'
        >>> ep.attr
        'attr'
        >>> ep.extras
        ['extra1', 'extra2']
        """

        pattern: ClassVar[Pattern[str]]

        def load(self) -> Any:  # Callable[[], Any] or an importable module
            """Load the entry point from its definition. If only a module
            is indicated by the value, return that module. Otherwise,
            return the named object.
            """

        @property
        def extras(self) -> list[str]: ...
        @property
        def module(self) -> str: ...
        @property
        def attr(self) -> str: ...
        if sys.version_info >= (3, 10):
            dist: ClassVar[Distribution | None]
            def matches(
                self,
                *,
                name: str = ...,
                value: str = ...,
                group: str = ...,
                module: str = ...,
                attr: str = ...,
                extras: list[str] = ...,
            ) -> bool:  # undocumented
                """
                EntryPoint matches the given parameters.

                >>> ep = EntryPoint(group='foo', name='bar', value='bing:bong [extra1, extra2]')
                >>> ep.matches(group='foo')
                True
                >>> ep.matches(name='bar', value='bing:bong [extra1, extra2]')
                True
                >>> ep.matches(group='foo', name='other')
                False
                >>> ep.matches()
                True
                >>> ep.matches(extras=['extra1', 'extra2'])
                True
                >>> ep.matches(module='bing')
                True
                >>> ep.matches(attr='bong')
                True
                """

        def __hash__(self) -> int: ...
        def __iter__(self) -> Iterator[Any]:  # result of iter((str, Self)), really
            """
            Supply iter so one may construct dicts of EntryPoints by name.
            """

if sys.version_info >= (3, 12):
    class EntryPoints(tuple[EntryPoint, ...]):
        """
        An immutable collection of selectable EntryPoint objects.
        """

        __slots__ = ()
        def __getitem__(self, name: str) -> EntryPoint:  # type: ignore[override]
            """
            Get the EntryPoint in self matching name.
            """

        def select(
            self,
            *,
            name: str = ...,
            value: str = ...,
            group: str = ...,
            module: str = ...,
            attr: str = ...,
            extras: list[str] = ...,
        ) -> EntryPoints:
            """
            Select entry points from self that match the
            given parameters (typically group and/or name).
            """

        @property
        def names(self) -> set[str]:
            """
            Return the set of all names of all entry points.
            """

        @property
        def groups(self) -> set[str]:
            """
            Return the set of all groups of all entry points.
            """

elif sys.version_info >= (3, 10):
    class DeprecatedList(list[_T]):
        """
        Allow an otherwise immutable object to implement mutability
        for compatibility.

        >>> recwarn = getfixture('recwarn')
        >>> dl = DeprecatedList(range(3))
        >>> dl[0] = 1
        >>> dl.append(3)
        >>> del dl[3]
        >>> dl.reverse()
        >>> dl.sort()
        >>> dl.extend([4])
        >>> dl.pop(-1)
        4
        >>> dl.remove(1)
        >>> dl += [5]
        >>> dl + [6]
        [1, 2, 5, 6]
        >>> dl + (6,)
        [1, 2, 5, 6]
        >>> dl.insert(0, 0)
        >>> dl
        [0, 1, 2, 5]
        >>> dl == [0, 1, 2, 5]
        True
        >>> dl == (0, 1, 2, 5)
        True
        >>> len(recwarn)
        1
        """

        __slots__ = ()

    class EntryPoints(DeprecatedList[EntryPoint]):  # use as list is deprecated since 3.10
        """
        An immutable collection of selectable EntryPoint objects.
        """

        # int argument is deprecated since 3.10
        __slots__ = ()
        def __getitem__(self, name: int | str) -> EntryPoint:  # type: ignore[override]
            """
            Get the EntryPoint in self matching name.
            """

        def select(
            self,
            *,
            name: str = ...,
            value: str = ...,
            group: str = ...,
            module: str = ...,
            attr: str = ...,
            extras: list[str] = ...,
        ) -> EntryPoints:
            """
            Select entry points from self that match the
            given parameters (typically group and/or name).
            """

        @property
        def names(self) -> set[str]:
            """
            Return the set of all names of all entry points.
            """

        @property
        def groups(self) -> set[str]:
            """
            Return the set of all groups of all entry points.

            For coverage while SelectableGroups is present.
            >>> EntryPoints().groups
            set()
            """

if sys.version_info >= (3, 10) and sys.version_info < (3, 12):
    class Deprecated(Generic[_KT, _VT]):
        """
        Compatibility add-in for mapping to indicate that
        mapping behavior is deprecated.

        >>> recwarn = getfixture('recwarn')
        >>> class DeprecatedDict(Deprecated, dict): pass
        >>> dd = DeprecatedDict(foo='bar')
        >>> dd.get('baz', None)
        >>> dd['foo']
        'bar'
        >>> list(dd)
        ['foo']
        >>> list(dd.keys())
        ['foo']
        >>> 'foo' in dd
        True
        >>> list(dd.values())
        ['bar']
        >>> len(recwarn)
        1
        """

        def __getitem__(self, name: _KT) -> _VT: ...
        @overload
        def get(self, name: _KT, default: None = None) -> _VT | None: ...
        @overload
        def get(self, name: _KT, default: _VT) -> _VT: ...
        @overload
        def get(self, name: _KT, default: _T) -> _VT | _T: ...
        def __iter__(self) -> Iterator[_KT]: ...
        def __contains__(self, *args: object) -> bool: ...
        def keys(self) -> dict_keys[_KT, _VT]: ...
        def values(self) -> dict_values[_KT, _VT]: ...

    @deprecated("Deprecated since Python 3.10; removed in Python 3.12. Use `select` instead.")
    class SelectableGroups(Deprecated[str, EntryPoints], dict[str, EntryPoints]):  # use as dict is deprecated since 3.10
        """
        A backward- and forward-compatible result from
        entry_points that fully implements the dict interface.
        """

        @classmethod
        def load(cls, eps: Iterable[EntryPoint]) -> Self: ...
        @property
        def groups(self) -> set[str]: ...
        @property
        def names(self) -> set[str]:
            """
            for coverage:
            >>> SelectableGroups().names
            set()
            """

        @overload
        def select(self) -> Self: ...
        @overload
        def select(
            self,
            *,
            name: str = ...,
            value: str = ...,
            group: str = ...,
            module: str = ...,
            attr: str = ...,
            extras: list[str] = ...,
        ) -> EntryPoints: ...

class PackagePath(pathlib.PurePosixPath):
    """A reference to a path in a package"""

    def read_text(self, encoding: str = "utf-8") -> str: ...
    def read_binary(self) -> bytes: ...
    def locate(self) -> PathLike[str]:
        """Return a path-like object for this path"""
    # The following attributes are not defined on PackagePath, but are dynamically added by Distribution.files:
    hash: FileHash | None
    size: int | None
    dist: Distribution

class FileHash:
    mode: str
    value: str
    def __init__(self, spec: str) -> None: ...

if sys.version_info >= (3, 12):
    class DeprecatedNonAbstract: ...
    _distribution_parent = DeprecatedNonAbstract
else:
    _distribution_parent = object

class Distribution(_distribution_parent):
    """
    An abstract Python distribution package.

    Custom providers may derive from this class and define
    the abstract methods to provide a concrete implementation
    for their environment. Some providers may opt to override
    the default implementation of some properties to bypass
    the file-reading mechanism.
    """

    @abc.abstractmethod
    def read_text(self, filename: str) -> str | None:
        """Attempt to load metadata file given by the name.

        Python distribution metadata is organized by blobs of text
        typically represented as "files" in the metadata directory
        (e.g. package-1.0.dist-info). These files include things
        like:

        - METADATA: The distribution metadata including fields
          like Name and Version and Description.
        - entry_points.txt: A series of entry points as defined in
          `the entry points spec <https://packaging.python.org/en/latest/specifications/entry-points/#file-format>`_.
        - RECORD: A record of files according to
          `this recording spec <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>`_.

        A package may provide any set of files, including those
        not listed here or none at all.

        :param filename: The name of the file in the distribution info.
        :return: The text if found, otherwise None.
        """

    @abc.abstractmethod
    def locate_file(self, path: StrPath) -> _SimplePath:
        """
        Given a path to a file in this distribution, return a SimplePath
        to it.
        """

    @classmethod
    def from_name(cls, name: str) -> Distribution:
        """Return the Distribution for the given package name.

        :param name: The name of the distribution package to search for.
        :return: The Distribution instance (or subclass thereof) for the named
            package, if found.
        :raises PackageNotFoundError: When the named package's distribution
            metadata cannot be found.
        :raises ValueError: When an invalid value is supplied for name.
        """

    @overload
    @classmethod
    def discover(cls, *, context: DistributionFinder.Context) -> Iterable[Distribution]:
        """Return an iterable of Distribution objects for all packages.

        Pass a ``context`` or pass keyword arguments for constructing
        a context.

        :context: A ``DistributionFinder.Context`` object.
        :return: Iterable of Distribution objects for packages matching
          the context.
        """

    @overload
    @classmethod
    def discover(
        cls, *, context: None = None, name: str | None = ..., path: list[str] = ..., **kwargs: Any
    ) -> Iterable[Distribution]: ...
    @staticmethod
    def at(path: StrPath) -> PathDistribution:
        """Return a Distribution for the indicated metadata path.

        :param path: a string or path-like object
        :return: a concrete Distribution instance for the path
        """
    if sys.version_info >= (3, 10):
        @property
        def metadata(self) -> PackageMetadata:
            """Return the parsed metadata for this Distribution.

            The returned object will have keys that name the various bits of
            metadata per the
            `Core metadata specifications <https://packaging.python.org/en/latest/specifications/core-metadata/#core-metadata>`_.

            Custom providers may provide the METADATA file or override this
            property.
            """

        @property
        def entry_points(self) -> EntryPoints:
            """
            Return EntryPoints for this distribution.

            Custom providers may provide the ``entry_points.txt`` file
            or override this property.
            """
    else:
        @property
        def metadata(self) -> Message:
            """Return the parsed metadata for this Distribution.

            The returned object will have keys that name the various bits of
            metadata.  See PEP 566 for details.
            """

        @property
        def entry_points(self) -> list[EntryPoint]: ...

    @property
    def version(self) -> str:
        """Return the 'Version' metadata for the distribution package."""

    @property
    def files(self) -> list[PackagePath] | None:
        """Files in this distribution.

        :return: List of PackagePath for this distribution or None

        Result is `None` if the metadata file that enumerates files
        (i.e. RECORD for dist-info, or installed-files.txt or
        SOURCES.txt for egg-info) is missing.
        Result may be empty if the metadata exists but is empty.

        Custom providers are recommended to provide a "RECORD" file (in
        ``read_text``) or override this property to allow for callers to be
        able to resolve filenames provided by the package.
        """

    @property
    def requires(self) -> list[str] | None:
        """Generated requirements specified for this Distribution"""
    if sys.version_info >= (3, 10):
        @property
        def name(self) -> str:
            """Return the 'Name' metadata for the distribution package."""
    if sys.version_info >= (3, 13):
        @property
        def origin(self) -> types.SimpleNamespace | None: ...

class DistributionFinder(MetaPathFinder):
    """
    A MetaPathFinder capable of discovering installed distributions.

    Custom providers should implement this interface in order to
    supply metadata.
    """

    class Context:
        """
        Keyword arguments presented by the caller to
        ``distributions()`` or ``Distribution.discover()``
        to narrow the scope of a search for distributions
        in all DistributionFinders.

        Each DistributionFinder may expect any parameters
        and should attempt to honor the canonical
        parameters defined below when appropriate.

        This mechanism gives a custom provider a means to
        solicit additional details from the caller beyond
        "name" and "path" when searching distributions.
        For example, imagine a provider that exposes suites
        of packages in either a "public" or "private" ``realm``.
        A caller may wish to query only for distributions in
        a particular realm and could call
        ``distributions(realm="private")`` to signal to the
        custom provider to only include distributions from that
        realm.
        """

        name: str | None
        def __init__(self, *, name: str | None = ..., path: list[str] = ..., **kwargs: Any) -> None: ...
        @property
        def path(self) -> list[str]:
            """
            The sequence of directory path that a distribution finder
            should search.

            Typically refers to Python installed package paths such as
            "site-packages" directories and defaults to ``sys.path``.
            """

    @abc.abstractmethod
    def find_distributions(self, context: DistributionFinder.Context = ...) -> Iterable[Distribution]:
        """
        Find distributions.

        Return an iterable of all Distribution instances capable of
        loading the metadata for packages matching the ``context``,
        a DistributionFinder.Context instance.
        """

class MetadataPathFinder(DistributionFinder):
    @classmethod
    def find_distributions(cls, context: DistributionFinder.Context = ...) -> Iterable[PathDistribution]:
        """
        Find distributions.

        Return an iterable of all Distribution instances capable of
        loading the metadata for packages matching ``context.name``
        (or all names if ``None`` indicated) along the paths in the list
        of directories ``context.path``.
        """
    if sys.version_info >= (3, 11):
        @classmethod
        def invalidate_caches(cls) -> None: ...
    elif sys.version_info >= (3, 10):
        # Yes, this is an instance method that has a parameter named "cls"
        def invalidate_caches(cls) -> None: ...

class PathDistribution(Distribution):
    _path: _SimplePath
    def __init__(self, path: _SimplePath) -> None:
        """Construct a distribution.

        :param path: SimplePath indicating the metadata directory.
        """

    def read_text(self, filename: StrPath) -> str | None:
        """Attempt to load metadata file given by the name.

        Python distribution metadata is organized by blobs of text
        typically represented as "files" in the metadata directory
        (e.g. package-1.0.dist-info). These files include things
        like:

        - METADATA: The distribution metadata including fields
          like Name and Version and Description.
        - entry_points.txt: A series of entry points as defined in
          `the entry points spec <https://packaging.python.org/en/latest/specifications/entry-points/#file-format>`_.
        - RECORD: A record of files according to
          `this recording spec <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>`_.

        A package may provide any set of files, including those
        not listed here or none at all.

        :param filename: The name of the file in the distribution info.
        :return: The text if found, otherwise None.
        """

    def locate_file(self, path: StrPath) -> _SimplePath: ...

def distribution(distribution_name: str) -> Distribution:
    """Get the ``Distribution`` instance for the named package.

    :param distribution_name: The name of the distribution package as a string.
    :return: A ``Distribution`` instance (or subclass thereof).
    """

@overload
def distributions(*, context: DistributionFinder.Context) -> Iterable[Distribution]:
    """Get all ``Distribution`` instances in the current environment.

    :return: An iterable of ``Distribution`` instances.
    """

@overload
def distributions(
    *, context: None = None, name: str | None = ..., path: list[str] = ..., **kwargs: Any
) -> Iterable[Distribution]: ...

if sys.version_info >= (3, 10):
    def metadata(distribution_name: str) -> PackageMetadata:
        """Get the metadata for the named package.

        :param distribution_name: The name of the distribution package to query.
        :return: A PackageMetadata containing the parsed metadata.
        """

else:
    def metadata(distribution_name: str) -> Message:
        """Get the metadata for the named package.

        :param distribution_name: The name of the distribution package to query.
        :return: An email.Message containing the parsed metadata.
        """

if sys.version_info >= (3, 12):
    def entry_points(
        *, name: str = ..., value: str = ..., group: str = ..., module: str = ..., attr: str = ..., extras: list[str] = ...
    ) -> EntryPoints:
        """Return EntryPoint objects for all installed packages.

        Pass selection parameters (group or name) to filter the
        result to entry points matching those properties (see
        EntryPoints.select()).

        :return: EntryPoints for all installed packages.
        """

elif sys.version_info >= (3, 10):
    @overload
    def entry_points() -> SelectableGroups:
        """Return EntryPoint objects for all installed packages.

        Pass selection parameters (group or name) to filter the
        result to entry points matching those properties (see
        EntryPoints.select()).

        For compatibility, returns ``SelectableGroups`` object unless
        selection parameters are supplied. In the future, this function
        will return ``EntryPoints`` instead of ``SelectableGroups``
        even when no selection parameters are supplied.

        For maximum future compatibility, pass selection parameters
        or invoke ``.select`` with parameters on the result.

        :return: EntryPoints or SelectableGroups for all installed packages.
        """

    @overload
    def entry_points(
        *, name: str = ..., value: str = ..., group: str = ..., module: str = ..., attr: str = ..., extras: list[str] = ...
    ) -> EntryPoints: ...

else:
    def entry_points() -> dict[str, list[EntryPoint]]:
        """Return EntryPoint objects for all installed packages.

        :return: EntryPoint objects for all installed packages.
        """

def version(distribution_name: str) -> str:
    """Get the version string for the named package.

    :param distribution_name: The name of the distribution package to query.
    :return: The version string for the package as defined in the package's
        "Version" metadata key.
    """

def files(distribution_name: str) -> list[PackagePath] | None:
    """Return a list of files for the named package.

    :param distribution_name: The name of the distribution package to query.
    :return: List of files composing the distribution.
    """

def requires(distribution_name: str) -> list[str] | None:
    """
    Return a list of requirements for the named package.

    :return: An iterable of requirements, suitable for
        packaging.requirement.Requirement.
    """
