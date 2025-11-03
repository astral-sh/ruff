"""Manage shelves of pickled objects.

A "shelf" is a persistent, dictionary-like object.  The difference
with dbm databases is that the values (not the keys!) in a shelf can
be essentially arbitrary Python objects -- anything that the "pickle"
module can handle.  This includes most class instances, recursive data
types, and objects containing lots of shared sub-objects.  The keys
are ordinary strings.

To summarize the interface (key is a string, data is an arbitrary
object):

        import shelve
        d = shelve.open(filename) # open, with (g)dbm filename -- no suffix

        d[key] = data   # store data at key (overwrites old data if
                        # using an existing key)
        data = d[key]   # retrieve a COPY of the data at key (raise
                        # KeyError if no such key) -- NOTE that this
                        # access returns a *copy* of the entry!
        del d[key]      # delete data stored at key (raises KeyError
                        # if no such key)
        flag = key in d # true if the key exists
        list = d.keys() # a list of all existing keys (slow!)

        d.close()       # close it

Dependent on the implementation, closing a persistent dictionary may
or may not be necessary to flush changes to disk.

Normally, d[key] returns a COPY of the entry.  This needs care when
mutable entries are mutated: for example, if d[key] is a list,
        d[key].append(anitem)
does NOT modify the entry d[key] itself, as stored in the persistent
mapping -- it only modifies the copy, which is then immediately
discarded, so that the append has NO effect whatsoever.  To append an
item to d[key] in a way that will affect the persistent mapping, use:
        data = d[key]
        data.append(anitem)
        d[key] = data

To avoid the problem with mutable entries, you may pass the keyword
argument writeback=True in the call to shelve.open.  When you use:
        d = shelve.open(filename, writeback=True)
then d keeps a cache of all entries you access, and writes them all back
to the persistent mapping when you call d.close().  This ensures that
such usage as d[key].append(anitem) works as intended.

However, using keyword argument writeback=True may consume vast amount
of memory for the cache, and it may make d.close() very slow, if you
access many of d's entries after opening it in this way: d has no way to
check which of the entries you access are mutable and/or which ones you
actually mutate, so it must cache, and write back at close, all of the
entries that you access.  You can call d.sync() to write back all the
entries in the cache, and empty the cache (d.sync() also synchronizes
the persistent dictionary on disk, if feasible).
"""

import sys
from _typeshed import StrOrBytesPath
from collections.abc import Iterator, MutableMapping
from dbm import _TFlags
from types import TracebackType
from typing import Any, TypeVar, overload
from typing_extensions import Self

__all__ = ["Shelf", "BsdDbShelf", "DbfilenameShelf", "open"]

_T = TypeVar("_T")
_VT = TypeVar("_VT")

class Shelf(MutableMapping[str, _VT]):
    """Base class for shelf implementations.

    This is initialized with a dictionary-like object.
    See the module's __doc__ string for an overview of the interface.
    """

    def __init__(
        self, dict: MutableMapping[bytes, bytes], protocol: int | None = None, writeback: bool = False, keyencoding: str = "utf-8"
    ) -> None: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...
    @overload  # type: ignore[override]
    def get(self, key: str, default: None = None) -> _VT | None: ...
    @overload
    def get(self, key: str, default: _VT) -> _VT: ...
    @overload
    def get(self, key: str, default: _T) -> _VT | _T: ...
    def __getitem__(self, key: str) -> _VT: ...
    def __setitem__(self, key: str, value: _VT) -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __contains__(self, key: str) -> bool: ...  # type: ignore[override]
    def __enter__(self) -> Self: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    def __del__(self) -> None: ...
    def close(self) -> None: ...
    def sync(self) -> None: ...

class BsdDbShelf(Shelf[_VT]):
    """Shelf implementation using the "BSD" db interface.

    This adds methods first(), next(), previous(), last() and
    set_location() that have no counterpart in [g]dbm databases.

    The actual database must be opened using one of the "bsddb"
    modules "open" routines (i.e. bsddb.hashopen, bsddb.btopen or
    bsddb.rnopen) and passed to the constructor.

    See the module's __doc__ string for an overview of the interface.
    """

    def set_location(self, key: str) -> tuple[str, _VT]: ...
    def next(self) -> tuple[str, _VT]: ...
    def previous(self) -> tuple[str, _VT]: ...
    def first(self) -> tuple[str, _VT]: ...
    def last(self) -> tuple[str, _VT]: ...

class DbfilenameShelf(Shelf[_VT]):
    """Shelf implementation using the "dbm" generic dbm interface.

    This is initialized with the filename for the dbm database.
    See the module's __doc__ string for an overview of the interface.
    """

    if sys.version_info >= (3, 11):
        def __init__(
            self, filename: StrOrBytesPath, flag: _TFlags = "c", protocol: int | None = None, writeback: bool = False
        ) -> None: ...
    else:
        def __init__(self, filename: str, flag: _TFlags = "c", protocol: int | None = None, writeback: bool = False) -> None: ...

if sys.version_info >= (3, 11):
    def open(filename: StrOrBytesPath, flag: _TFlags = "c", protocol: int | None = None, writeback: bool = False) -> Shelf[Any]:
        """Open a persistent dictionary for reading and writing.

        The filename parameter is the base filename for the underlying
        database.  As a side-effect, an extension may be added to the
        filename and more than one file may be created.  The optional flag
        parameter has the same interpretation as the flag parameter of
        dbm.open(). The optional protocol parameter specifies the
        version of the pickle protocol.

        See the module's __doc__ string for an overview of the interface.
        """

else:
    def open(filename: str, flag: _TFlags = "c", protocol: int | None = None, writeback: bool = False) -> Shelf[Any]:
        """Open a persistent dictionary for reading and writing.

        The filename parameter is the base filename for the underlying
        database.  As a side-effect, an extension may be added to the
        filename and more than one file may be created.  The optional flag
        parameter has the same interpretation as the flag parameter of
        dbm.open(). The optional protocol parameter specifies the
        version of the pickle protocol.

        See the module's __doc__ string for an overview of the interface.
        """
