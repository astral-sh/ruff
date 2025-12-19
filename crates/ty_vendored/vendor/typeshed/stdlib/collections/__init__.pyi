"""This module implements specialized container datatypes providing
alternatives to Python's general purpose built-in containers, dict,
list, set, and tuple.

* namedtuple   factory function for creating tuple subclasses with named fields
* deque        list-like container with fast appends and pops on either end
* ChainMap     dict-like class for creating a single view of multiple mappings
* Counter      dict subclass for counting hashable objects
* OrderedDict  dict subclass that remembers the order entries were added
* defaultdict  dict subclass that calls a factory function to supply missing values
* UserDict     wrapper around dictionary objects for easier dict subclassing
* UserList     wrapper around list objects for easier list subclassing
* UserString   wrapper around string objects for easier string subclassing

"""

import sys
from _collections_abc import dict_items, dict_keys, dict_values
from _typeshed import SupportsItems, SupportsKeysAndGetItem, SupportsRichComparison, SupportsRichComparisonT
from types import GenericAlias
from typing import Any, ClassVar, Generic, NoReturn, SupportsIndex, TypeVar, final, overload, type_check_only
from typing_extensions import Self, disjoint_base

if sys.version_info >= (3, 10):
    from collections.abc import (
        Callable,
        ItemsView,
        Iterable,
        Iterator,
        KeysView,
        Mapping,
        MutableMapping,
        MutableSequence,
        Sequence,
        ValuesView,
    )
else:
    from _collections_abc import *

__all__ = ["ChainMap", "Counter", "OrderedDict", "UserDict", "UserList", "UserString", "defaultdict", "deque", "namedtuple"]

_S = TypeVar("_S")
_T = TypeVar("_T")
_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")
_KT = TypeVar("_KT")
_VT = TypeVar("_VT")
_KT_co = TypeVar("_KT_co", covariant=True)
_VT_co = TypeVar("_VT_co", covariant=True)

# namedtuple is special-cased in the type checker; the initializer is ignored.
def namedtuple(
    typename: str,
    field_names: str | Iterable[str],
    *,
    rename: bool = False,
    module: str | None = None,
    defaults: Iterable[Any] | None = None,
) -> type[tuple[Any, ...]]:
    """Returns a new subclass of tuple with named fields.

    >>> Point = namedtuple('Point', ['x', 'y'])
    >>> Point.__doc__                   # docstring for the new class
    'Point(x, y)'
    >>> p = Point(11, y=22)             # instantiate with positional args or keywords
    >>> p[0] + p[1]                     # indexable like a plain tuple
    33
    >>> x, y = p                        # unpack like a regular tuple
    >>> x, y
    (11, 22)
    >>> p.x + p.y                       # fields also accessible by name
    33
    >>> d = p._asdict()                 # convert to a dictionary
    >>> d['x']
    11
    >>> Point(**d)                      # convert from a dictionary
    Point(x=11, y=22)
    >>> p._replace(x=100)               # _replace() is like str.replace() but targets named fields
    Point(x=100, y=22)

    """

class UserDict(MutableMapping[_KT, _VT]):
    data: dict[_KT, _VT]
    # __init__ should be kept roughly in line with `dict.__init__`, which has the same semantics
    @overload
    def __init__(self, dict: None = None, /) -> None: ...
    @overload
    def __init__(
        self: UserDict[str, _VT], dict: None = None, /, **kwargs: _VT  # pyright: ignore[reportInvalidTypeVarUse]  #11780
    ) -> None: ...
    @overload
    def __init__(self, dict: SupportsKeysAndGetItem[_KT, _VT], /) -> None: ...
    @overload
    def __init__(
        self: UserDict[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        dict: SupportsKeysAndGetItem[str, _VT],
        /,
        **kwargs: _VT,
    ) -> None: ...
    @overload
    def __init__(self, iterable: Iterable[tuple[_KT, _VT]], /) -> None: ...
    @overload
    def __init__(
        self: UserDict[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        iterable: Iterable[tuple[str, _VT]],
        /,
        **kwargs: _VT,
    ) -> None: ...
    @overload
    def __init__(self: UserDict[str, str], iterable: Iterable[list[str]], /) -> None: ...
    @overload
    def __init__(self: UserDict[bytes, bytes], iterable: Iterable[list[bytes]], /) -> None: ...
    def __len__(self) -> int: ...
    def __getitem__(self, key: _KT) -> _VT: ...
    def __setitem__(self, key: _KT, item: _VT) -> None: ...
    def __delitem__(self, key: _KT) -> None: ...
    def __iter__(self) -> Iterator[_KT]: ...
    def __contains__(self, key: object) -> bool: ...
    def copy(self) -> Self: ...
    def __copy__(self) -> Self: ...

    # `UserDict.fromkeys` has the same semantics as `dict.fromkeys`, so should be kept in line with `dict.fromkeys`.
    # TODO: Much like `dict.fromkeys`, the true signature of `UserDict.fromkeys` is inexpressible in the current type system.
    # See #3800 & https://github.com/python/typing/issues/548#issuecomment-683336963.
    @classmethod
    @overload
    def fromkeys(cls, iterable: Iterable[_T], value: None = None) -> UserDict[_T, Any | None]: ...
    @classmethod
    @overload
    def fromkeys(cls, iterable: Iterable[_T], value: _S) -> UserDict[_T, _S]: ...
    @overload
    def __or__(self, other: UserDict[_KT, _VT] | dict[_KT, _VT]) -> Self: ...
    @overload
    def __or__(self, other: UserDict[_T1, _T2] | dict[_T1, _T2]) -> UserDict[_KT | _T1, _VT | _T2]: ...
    @overload
    def __ror__(self, other: UserDict[_KT, _VT] | dict[_KT, _VT]) -> Self: ...
    @overload
    def __ror__(self, other: UserDict[_T1, _T2] | dict[_T1, _T2]) -> UserDict[_KT | _T1, _VT | _T2]: ...
    # UserDict.__ior__ should be kept roughly in line with MutableMapping.update()
    @overload  # type: ignore[misc]
    def __ior__(self, other: SupportsKeysAndGetItem[_KT, _VT]) -> Self: ...
    @overload
    def __ior__(self, other: Iterable[tuple[_KT, _VT]]) -> Self: ...
    if sys.version_info >= (3, 12):
        @overload
        def get(self, key: _KT, default: None = None) -> _VT | None: ...
        @overload
        def get(self, key: _KT, default: _VT) -> _VT: ...
        @overload
        def get(self, key: _KT, default: _T) -> _VT | _T: ...

class UserList(MutableSequence[_T]):
    """A more or less complete user-defined wrapper around list objects."""

    data: list[_T]
    @overload
    def __init__(self, initlist: None = None) -> None: ...
    @overload
    def __init__(self, initlist: Iterable[_T]) -> None: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __lt__(self, other: list[_T] | UserList[_T]) -> bool: ...
    def __le__(self, other: list[_T] | UserList[_T]) -> bool: ...
    def __gt__(self, other: list[_T] | UserList[_T]) -> bool: ...
    def __ge__(self, other: list[_T] | UserList[_T]) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __contains__(self, item: object) -> bool: ...
    def __len__(self) -> int: ...
    @overload
    def __getitem__(self, i: SupportsIndex) -> _T: ...
    @overload
    def __getitem__(self, i: slice) -> Self: ...
    @overload
    def __setitem__(self, i: SupportsIndex, item: _T) -> None: ...
    @overload
    def __setitem__(self, i: slice, item: Iterable[_T]) -> None: ...
    def __delitem__(self, i: SupportsIndex | slice) -> None: ...
    def __add__(self, other: Iterable[_T]) -> Self: ...
    def __radd__(self, other: Iterable[_T]) -> Self: ...
    def __iadd__(self, other: Iterable[_T]) -> Self: ...
    def __mul__(self, n: int) -> Self: ...
    def __rmul__(self, n: int) -> Self: ...
    def __imul__(self, n: int) -> Self: ...
    def append(self, item: _T) -> None: ...
    def insert(self, i: int, item: _T) -> None: ...
    def pop(self, i: int = -1) -> _T: ...
    def remove(self, item: _T) -> None: ...
    def copy(self) -> Self: ...
    def __copy__(self) -> Self: ...
    def count(self, item: _T) -> int: ...
    # The runtime signature is "item, *args", and the arguments are then passed
    # to `list.index`. In order to give more precise types, we pretend that the
    # `item` argument is positional-only.
    def index(self, item: _T, start: SupportsIndex = 0, stop: SupportsIndex = sys.maxsize, /) -> int: ...
    # All arguments are passed to `list.sort` at runtime, so the signature should be kept in line with `list.sort`.
    @overload
    def sort(self: UserList[SupportsRichComparisonT], *, key: None = None, reverse: bool = False) -> None: ...
    @overload
    def sort(self, *, key: Callable[[_T], SupportsRichComparison], reverse: bool = False) -> None: ...
    def extend(self, other: Iterable[_T]) -> None: ...

class UserString(Sequence[UserString]):
    data: str
    def __init__(self, seq: object) -> None: ...
    def __int__(self) -> int: ...
    def __float__(self) -> float: ...
    def __complex__(self) -> complex: ...
    def __getnewargs__(self) -> tuple[str]: ...
    def __lt__(self, string: str | UserString) -> bool: ...
    def __le__(self, string: str | UserString) -> bool: ...
    def __gt__(self, string: str | UserString) -> bool: ...
    def __ge__(self, string: str | UserString) -> bool: ...
    def __eq__(self, string: object) -> bool: ...
    def __hash__(self) -> int: ...
    def __contains__(self, char: object) -> bool: ...
    def __len__(self) -> int: ...
    def __getitem__(self, index: SupportsIndex | slice) -> Self: ...
    def __iter__(self) -> Iterator[Self]: ...
    def __reversed__(self) -> Iterator[Self]: ...
    def __add__(self, other: object) -> Self: ...
    def __radd__(self, other: object) -> Self: ...
    def __mul__(self, n: int) -> Self: ...
    def __rmul__(self, n: int) -> Self: ...
    def __mod__(self, args: Any) -> Self: ...
    def __rmod__(self, template: object) -> Self: ...
    def capitalize(self) -> Self: ...
    def casefold(self) -> Self: ...
    def center(self, width: int, *args: Any) -> Self: ...
    def count(self, sub: str | UserString, start: int = 0, end: int = sys.maxsize) -> int: ...
    def encode(self: UserString, encoding: str | None = "utf-8", errors: str | None = "strict") -> bytes: ...
    def endswith(self, suffix: str | tuple[str, ...], start: int | None = 0, end: int | None = sys.maxsize) -> bool: ...
    def expandtabs(self, tabsize: int = 8) -> Self: ...
    def find(self, sub: str | UserString, start: int = 0, end: int = sys.maxsize) -> int: ...
    def format(self, *args: Any, **kwds: Any) -> str: ...
    def format_map(self, mapping: Mapping[str, Any]) -> str: ...
    def index(self, sub: str, start: int = 0, end: int = sys.maxsize) -> int: ...
    def isalpha(self) -> bool: ...
    def isalnum(self) -> bool: ...
    def isdecimal(self) -> bool: ...
    def isdigit(self) -> bool: ...
    def isidentifier(self) -> bool: ...
    def islower(self) -> bool: ...
    def isnumeric(self) -> bool: ...
    def isprintable(self) -> bool: ...
    def isspace(self) -> bool: ...
    def istitle(self) -> bool: ...
    def isupper(self) -> bool: ...
    def isascii(self) -> bool: ...
    def join(self, seq: Iterable[str]) -> str: ...
    def ljust(self, width: int, *args: Any) -> Self: ...
    def lower(self) -> Self: ...
    def lstrip(self, chars: str | None = None) -> Self: ...
    maketrans = str.maketrans
    def partition(self, sep: str) -> tuple[str, str, str]: ...
    def removeprefix(self, prefix: str | UserString, /) -> Self: ...
    def removesuffix(self, suffix: str | UserString, /) -> Self: ...
    def replace(self, old: str | UserString, new: str | UserString, maxsplit: int = -1) -> Self: ...
    def rfind(self, sub: str | UserString, start: int = 0, end: int = sys.maxsize) -> int: ...
    def rindex(self, sub: str | UserString, start: int = 0, end: int = sys.maxsize) -> int: ...
    def rjust(self, width: int, *args: Any) -> Self: ...
    def rpartition(self, sep: str) -> tuple[str, str, str]: ...
    def rstrip(self, chars: str | None = None) -> Self: ...
    def split(self, sep: str | None = None, maxsplit: int = -1) -> list[str]: ...
    def rsplit(self, sep: str | None = None, maxsplit: int = -1) -> list[str]: ...
    def splitlines(self, keepends: bool = False) -> list[str]: ...
    def startswith(self, prefix: str | tuple[str, ...], start: int | None = 0, end: int | None = sys.maxsize) -> bool: ...
    def strip(self, chars: str | None = None) -> Self: ...
    def swapcase(self) -> Self: ...
    def title(self) -> Self: ...
    def translate(self, *args: Any) -> Self: ...
    def upper(self) -> Self: ...
    def zfill(self, width: int) -> Self: ...

@disjoint_base
class deque(MutableSequence[_T]):
    """A list-like sequence optimized for data accesses near its endpoints."""

    @property
    def maxlen(self) -> int | None:
        """maximum size of a deque or None if unbounded"""

    @overload
    def __init__(self, *, maxlen: int | None = None) -> None: ...
    @overload
    def __init__(self, iterable: Iterable[_T], maxlen: int | None = None) -> None: ...
    def append(self, x: _T, /) -> None:
        """Add an element to the right side of the deque."""

    def appendleft(self, x: _T, /) -> None:
        """Add an element to the left side of the deque."""

    def copy(self) -> Self:
        """Return a shallow copy of a deque."""

    def count(self, x: _T, /) -> int:
        """Return number of occurrences of value."""

    def extend(self, iterable: Iterable[_T], /) -> None:
        """Extend the right side of the deque with elements from the iterable."""

    def extendleft(self, iterable: Iterable[_T], /) -> None:
        """Extend the left side of the deque with elements from the iterable."""

    def insert(self, i: int, x: _T, /) -> None:
        """Insert value before index."""

    def index(self, x: _T, start: int = 0, stop: int = ..., /) -> int:
        """Return first index of value.

        Raises ValueError if the value is not present.
        """

    def pop(self) -> _T:  # type: ignore[override]
        """Remove and return the rightmost element."""

    def popleft(self) -> _T:
        """Remove and return the leftmost element."""

    def remove(self, value: _T, /) -> None:
        """Remove first occurrence of value."""

    def rotate(self, n: int = 1, /) -> None:
        """Rotate the deque n steps to the right.  If n is negative, rotates left."""

    def __copy__(self) -> Self:
        """Return a shallow copy of a deque."""

    def __len__(self) -> int:
        """Return len(self)."""
    __hash__: ClassVar[None]  # type: ignore[assignment]
    # These methods of deque don't take slices, unlike MutableSequence, hence the type: ignores
    def __getitem__(self, key: SupportsIndex, /) -> _T:  # type: ignore[override]
        """Return self[key]."""

    def __setitem__(self, key: SupportsIndex, value: _T, /) -> None:  # type: ignore[override]
        """Set self[key] to value."""

    def __delitem__(self, key: SupportsIndex, /) -> None:  # type: ignore[override]
        """Delete self[key]."""

    def __contains__(self, key: object, /) -> bool:
        """Return bool(key in self)."""

    def __reduce__(self) -> tuple[type[Self], tuple[()], None, Iterator[_T]]:
        """Return state information for pickling."""

    def __iadd__(self, value: Iterable[_T], /) -> Self:
        """Implement self+=value."""

    def __add__(self, value: Self, /) -> Self:
        """Return self+value."""

    def __mul__(self, value: int, /) -> Self:
        """Return self*value."""

    def __imul__(self, value: int, /) -> Self:
        """Implement self*=value."""

    def __lt__(self, value: deque[_T], /) -> bool: ...
    def __le__(self, value: deque[_T], /) -> bool: ...
    def __gt__(self, value: deque[_T], /) -> bool: ...
    def __ge__(self, value: deque[_T], /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

class Counter(dict[_T, int], Generic[_T]):
    """Dict subclass for counting hashable items.  Sometimes called a bag
    or multiset.  Elements are stored as dictionary keys and their counts
    are stored as dictionary values.

    >>> c = Counter('abcdeabcdabcaba')  # count elements from a string

    >>> c.most_common(3)                # three most common elements
    [('a', 5), ('b', 4), ('c', 3)]
    >>> sorted(c)                       # list all unique elements
    ['a', 'b', 'c', 'd', 'e']
    >>> ''.join(sorted(c.elements()))   # list elements with repetitions
    'aaaaabbbbcccdde'
    >>> sum(c.values())                 # total of all counts
    15

    >>> c['a']                          # count of letter 'a'
    5
    >>> for elem in 'shazam':           # update counts from an iterable
    ...     c[elem] += 1                # by adding 1 to each element's count
    >>> c['a']                          # now there are seven 'a'
    7
    >>> del c['b']                      # remove all 'b'
    >>> c['b']                          # now there are zero 'b'
    0

    >>> d = Counter('simsalabim')       # make another counter
    >>> c.update(d)                     # add in the second counter
    >>> c['a']                          # now there are nine 'a'
    9

    >>> c.clear()                       # empty the counter
    >>> c
    Counter()

    Note:  If a count is set to zero or reduced to zero, it will remain
    in the counter until the entry is deleted or the counter is cleared:

    >>> c = Counter('aaabbc')
    >>> c['b'] -= 2                     # reduce the count of 'b' by two
    >>> c.most_common()                 # 'b' is still in, but its count is zero
    [('a', 3), ('c', 1), ('b', 0)]

    """

    @overload
    def __init__(self, iterable: None = None, /) -> None:
        """Create a new, empty Counter object.  And if given, count elements
        from an input iterable.  Or, initialize the count from another mapping
        of elements to their counts.

        >>> c = Counter()                           # a new, empty counter
        >>> c = Counter('gallahad')                 # a new counter from an iterable
        >>> c = Counter({'a': 4, 'b': 2})           # a new counter from a mapping
        >>> c = Counter(a=4, b=2)                   # a new counter from keyword args

        """

    @overload
    def __init__(self: Counter[str], iterable: None = None, /, **kwargs: int) -> None: ...
    @overload
    def __init__(self, mapping: SupportsKeysAndGetItem[_T, int], /) -> None: ...
    @overload
    def __init__(self, iterable: Iterable[_T], /) -> None: ...
    def copy(self) -> Self:
        """Return a shallow copy."""

    def elements(self) -> Iterator[_T]:
        """Iterator over elements repeating each as many times as its count.

        >>> c = Counter('ABCABC')
        >>> sorted(c.elements())
        ['A', 'A', 'B', 'B', 'C', 'C']

        Knuth's example for prime factors of 1836:  2**2 * 3**3 * 17**1

        >>> import math
        >>> prime_factors = Counter({2: 2, 3: 3, 17: 1})
        >>> math.prod(prime_factors.elements())
        1836

        Note, if an element's count has been set to zero or is a negative
        number, elements() will ignore it.

        """

    def most_common(self, n: int | None = None) -> list[tuple[_T, int]]:
        """List the n most common elements and their counts from the most
        common to the least.  If n is None, then list all element counts.

        >>> Counter('abracadabra').most_common(3)
        [('a', 5), ('b', 2), ('r', 2)]

        """

    @classmethod
    def fromkeys(cls, iterable: Any, v: int | None = None) -> NoReturn: ...  # type: ignore[override]
    @overload
    def subtract(self, iterable: None = None, /) -> None:
        """Like dict.update() but subtracts counts instead of replacing them.
        Counts can be reduced below zero.  Both the inputs and outputs are
        allowed to contain zero and negative counts.

        Source can be an iterable, a dictionary, or another Counter instance.

        >>> c = Counter('which')
        >>> c.subtract('witch')             # subtract elements from another iterable
        >>> c.subtract(Counter('watch'))    # subtract elements from another counter
        >>> c['h']                          # 2 in which, minus 1 in witch, minus 1 in watch
        0
        >>> c['w']                          # 1 in which, minus 1 in witch, minus 1 in watch
        -1

        """

    @overload
    def subtract(self, mapping: Mapping[_T, int], /) -> None: ...
    @overload
    def subtract(self, iterable: Iterable[_T], /) -> None: ...
    # Unlike dict.update(), use Mapping instead of SupportsKeysAndGetItem for the first overload
    # (source code does an `isinstance(other, Mapping)` check)
    #
    # The second overload is also deliberately different to dict.update()
    # (if it were `Iterable[_T] | Iterable[tuple[_T, int]]`,
    # the tuples would be added as keys, breaking type safety)
    @overload  # type: ignore[override]
    def update(self, m: Mapping[_T, int], /, **kwargs: int) -> None:
        """Like dict.update() but add counts instead of replacing them.

        Source can be an iterable, a dictionary, or another Counter instance.

        >>> c = Counter('which')
        >>> c.update('witch')           # add elements from another iterable
        >>> d = Counter('watch')
        >>> c.update(d)                 # add elements from another counter
        >>> c['h']                      # four 'h' in which, witch, and watch
        4

        """

    @overload
    def update(self, iterable: Iterable[_T], /, **kwargs: int) -> None: ...
    @overload
    def update(self, iterable: None = None, /, **kwargs: int) -> None: ...
    def __missing__(self, key: _T) -> int:
        """The count of elements not in the Counter is zero."""

    def __delitem__(self, elem: object) -> None:
        """Like dict.__delitem__() but does not raise KeyError for missing values."""
    if sys.version_info >= (3, 10):
        def __eq__(self, other: object) -> bool:
            """True if all counts agree. Missing counts are treated as zero."""

        def __ne__(self, other: object) -> bool:
            """True if any counts disagree. Missing counts are treated as zero."""

    def __add__(self, other: Counter[_S]) -> Counter[_T | _S]:
        """Add counts from two counters.

        >>> Counter('abbb') + Counter('bcc')
        Counter({'b': 4, 'c': 2, 'a': 1})

        """

    def __sub__(self, other: Counter[_T]) -> Counter[_T]:
        """Subtract count, but keep only results with positive counts.

        >>> Counter('abbbc') - Counter('bccd')
        Counter({'b': 2, 'a': 1})

        """

    def __and__(self, other: Counter[_T]) -> Counter[_T]:
        """Intersection is the minimum of corresponding counts.

        >>> Counter('abbb') & Counter('bcc')
        Counter({'b': 1})

        """

    def __or__(self, other: Counter[_S]) -> Counter[_T | _S]:  # type: ignore[override]
        """Union is the maximum of value in either of the input counters.

        >>> Counter('abbb') | Counter('bcc')
        Counter({'b': 3, 'c': 2, 'a': 1})

        """

    def __pos__(self) -> Counter[_T]:
        """Adds an empty counter, effectively stripping negative and zero counts"""

    def __neg__(self) -> Counter[_T]:
        """Subtracts from an empty counter.  Strips positive and zero counts,
        and flips the sign on negative counts.

        """
    # several type: ignores because __iadd__ is supposedly incompatible with __add__, etc.
    def __iadd__(self, other: SupportsItems[_T, int]) -> Self:  # type: ignore[misc]
        """Inplace add from another counter, keeping only positive counts.

        >>> c = Counter('abbb')
        >>> c += Counter('bcc')
        >>> c
        Counter({'b': 4, 'c': 2, 'a': 1})

        """

    def __isub__(self, other: SupportsItems[_T, int]) -> Self:
        """Inplace subtract counter, but keep only results with positive counts.

        >>> c = Counter('abbbc')
        >>> c -= Counter('bccd')
        >>> c
        Counter({'b': 2, 'a': 1})

        """

    def __iand__(self, other: SupportsItems[_T, int]) -> Self:
        """Inplace intersection is the minimum of corresponding counts.

        >>> c = Counter('abbb')
        >>> c &= Counter('bcc')
        >>> c
        Counter({'b': 1})

        """

    def __ior__(self, other: SupportsItems[_T, int]) -> Self:  # type: ignore[override,misc]
        """Inplace union is the maximum of value from either counter.

        >>> c = Counter('abbb')
        >>> c |= Counter('bcc')
        >>> c
        Counter({'b': 3, 'c': 2, 'a': 1})

        """
    if sys.version_info >= (3, 10):
        def total(self) -> int:
            """Sum of the counts"""

        def __le__(self, other: Counter[Any]) -> bool:
            """True if all counts in self are a subset of those in other."""

        def __lt__(self, other: Counter[Any]) -> bool:
            """True if all counts in self are a proper subset of those in other."""

        def __ge__(self, other: Counter[Any]) -> bool:
            """True if all counts in self are a superset of those in other."""

        def __gt__(self, other: Counter[Any]) -> bool:
            """True if all counts in self are a proper superset of those in other."""

# The pure-Python implementations of the "views" classes
# These are exposed at runtime in `collections/__init__.py`
class _OrderedDictKeysView(KeysView[_KT_co]):
    def __reversed__(self) -> Iterator[_KT_co]: ...

class _OrderedDictItemsView(ItemsView[_KT_co, _VT_co]):
    def __reversed__(self) -> Iterator[tuple[_KT_co, _VT_co]]: ...

class _OrderedDictValuesView(ValuesView[_VT_co]):
    def __reversed__(self) -> Iterator[_VT_co]: ...

# The C implementations of the "views" classes
# (At runtime, these are called `odict_keys`, `odict_items` and `odict_values`,
# but they are not exposed anywhere)
# pyright doesn't have a specific error code for subclassing error!
@final
@type_check_only
class _odict_keys(dict_keys[_KT_co, _VT_co]):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    def __reversed__(self) -> Iterator[_KT_co]: ...

@final
@type_check_only
class _odict_items(dict_items[_KT_co, _VT_co]):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    def __reversed__(self) -> Iterator[tuple[_KT_co, _VT_co]]: ...

@final
@type_check_only
class _odict_values(dict_values[_KT_co, _VT_co]):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    def __reversed__(self) -> Iterator[_VT_co]: ...

@disjoint_base
class OrderedDict(dict[_KT, _VT]):
    """Dictionary that remembers insertion order"""

    def popitem(self, last: bool = True) -> tuple[_KT, _VT]:
        """Remove and return a (key, value) pair from the dictionary.

        Pairs are returned in LIFO order if last is true or FIFO order if false.
        """

    def move_to_end(self, key: _KT, last: bool = True) -> None:
        """Move an existing element to the end (or beginning if last is false).

        Raise KeyError if the element does not exist.
        """

    def copy(self) -> Self:
        """A shallow copy of ordered dict."""

    def __reversed__(self) -> Iterator[_KT]:
        """od.__reversed__() <==> reversed(od)"""

    def keys(self) -> _odict_keys[_KT, _VT]: ...
    def items(self) -> _odict_items[_KT, _VT]: ...
    def values(self) -> _odict_values[_KT, _VT]: ...
    # The signature of OrderedDict.fromkeys should be kept in line with `dict.fromkeys`, modulo positional-only differences.
    # Like dict.fromkeys, its true signature is not expressible in the current type system.
    # See #3800 & https://github.com/python/typing/issues/548#issuecomment-683336963.
    @classmethod
    @overload
    def fromkeys(cls, iterable: Iterable[_T], value: None = None) -> OrderedDict[_T, Any | None]:
        """Create a new ordered dictionary with keys from iterable and values set to value."""

    @classmethod
    @overload
    def fromkeys(cls, iterable: Iterable[_T], value: _S) -> OrderedDict[_T, _S]: ...
    # Keep OrderedDict.setdefault in line with MutableMapping.setdefault, modulo positional-only differences.
    @overload
    def setdefault(self: OrderedDict[_KT, _T | None], key: _KT, default: None = None) -> _T | None:
        """Insert key with a value of default if key is not in the dictionary.

        Return the value for key if key is in the dictionary, else default.
        """

    @overload
    def setdefault(self, key: _KT, default: _VT) -> _VT: ...
    # Same as dict.pop, but accepts keyword arguments
    @overload
    def pop(self, key: _KT) -> _VT:
        """od.pop(key[,default]) -> v, remove specified key and return the corresponding value.

        If the key is not found, return the default if given; otherwise,
        raise a KeyError.
        """

    @overload
    def pop(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _T) -> _VT | _T: ...
    def __eq__(self, value: object, /) -> bool: ...
    @overload
    def __or__(self, value: dict[_KT, _VT], /) -> Self:
        """Return self|value."""

    @overload
    def __or__(self, value: dict[_T1, _T2], /) -> OrderedDict[_KT | _T1, _VT | _T2]: ...
    @overload
    def __ror__(self, value: dict[_KT, _VT], /) -> Self:
        """Return value|self."""

    @overload
    def __ror__(self, value: dict[_T1, _T2], /) -> OrderedDict[_KT | _T1, _VT | _T2]: ...  # type: ignore[misc]

@disjoint_base
class defaultdict(dict[_KT, _VT]):
    """defaultdict(default_factory=None, /, [...]) --> dict with default factory

    The default factory is called without arguments to produce
    a new value when a key is not present, in __getitem__ only.
    A defaultdict compares equal to a dict with the same items.
    All remaining arguments are treated the same as if they were
    passed to the dict constructor, including keyword arguments.
    """

    default_factory: Callable[[], _VT] | None
    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self: defaultdict[str, _VT], **kwargs: _VT) -> None: ...  # pyright: ignore[reportInvalidTypeVarUse]  #11780
    @overload
    def __init__(self, default_factory: Callable[[], _VT] | None, /) -> None: ...
    @overload
    def __init__(
        self: defaultdict[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        default_factory: Callable[[], _VT] | None,
        /,
        **kwargs: _VT,
    ) -> None: ...
    @overload
    def __init__(self, default_factory: Callable[[], _VT] | None, map: SupportsKeysAndGetItem[_KT, _VT], /) -> None: ...
    @overload
    def __init__(
        self: defaultdict[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        default_factory: Callable[[], _VT] | None,
        map: SupportsKeysAndGetItem[str, _VT],
        /,
        **kwargs: _VT,
    ) -> None: ...
    @overload
    def __init__(self, default_factory: Callable[[], _VT] | None, iterable: Iterable[tuple[_KT, _VT]], /) -> None: ...
    @overload
    def __init__(
        self: defaultdict[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        default_factory: Callable[[], _VT] | None,
        iterable: Iterable[tuple[str, _VT]],
        /,
        **kwargs: _VT,
    ) -> None: ...
    def __missing__(self, key: _KT, /) -> _VT:
        """__missing__(key) # Called by __getitem__ for missing key; pseudo-code:
        if self.default_factory is None: raise KeyError((key,))
        self[key] = value = self.default_factory()
        return value
        """

    def __copy__(self) -> Self:
        """D.copy() -> a shallow copy of D."""

    def copy(self) -> Self:
        """D.copy() -> a shallow copy of D."""

    @overload
    def __or__(self, value: dict[_KT, _VT], /) -> Self:
        """Return self|value."""

    @overload
    def __or__(self, value: dict[_T1, _T2], /) -> defaultdict[_KT | _T1, _VT | _T2]: ...
    @overload
    def __ror__(self, value: dict[_KT, _VT], /) -> Self:
        """Return value|self."""

    @overload
    def __ror__(self, value: dict[_T1, _T2], /) -> defaultdict[_KT | _T1, _VT | _T2]: ...  # type: ignore[misc]

class ChainMap(MutableMapping[_KT, _VT]):
    """A ChainMap groups multiple dicts (or other mappings) together
    to create a single, updateable view.

    The underlying mappings are stored in a list.  That list is public and can
    be accessed or updated using the *maps* attribute.  There is no other
    state.

    Lookups search the underlying mappings successively until a key is found.
    In contrast, writes, updates, and deletions only operate on the first
    mapping.

    """

    maps: list[MutableMapping[_KT, _VT]]
    def __init__(self, *maps: MutableMapping[_KT, _VT]) -> None:
        """Initialize a ChainMap by setting *maps* to the given mappings.
        If no mappings are provided, a single empty dictionary is used.

        """

    def new_child(self, m: MutableMapping[_KT, _VT] | None = None) -> Self:
        """New ChainMap with a new map followed by all previous maps.
        If no map is provided, an empty dict is used.
        Keyword arguments update the map or new empty dict.
        """

    @property
    def parents(self) -> Self:
        """New ChainMap from maps[1:]."""

    def __setitem__(self, key: _KT, value: _VT) -> None: ...
    def __delitem__(self, key: _KT) -> None: ...
    def __getitem__(self, key: _KT) -> _VT: ...
    def __iter__(self) -> Iterator[_KT]: ...
    def __len__(self) -> int: ...
    def __contains__(self, key: object) -> bool: ...
    @overload
    def get(self, key: _KT, default: None = None) -> _VT | None: ...
    @overload
    def get(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def get(self, key: _KT, default: _T) -> _VT | _T: ...
    def __missing__(self, key: _KT) -> _VT: ...  # undocumented
    def __bool__(self) -> bool: ...
    # Keep ChainMap.setdefault in line with MutableMapping.setdefault, modulo positional-only differences.
    @overload
    def setdefault(self: ChainMap[_KT, _T | None], key: _KT, default: None = None) -> _T | None:
        """D.setdefault(k[,d]) -> D.get(k,d), also set D[k]=d if k not in D"""

    @overload
    def setdefault(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def pop(self, key: _KT) -> _VT:
        """Remove *key* from maps[0] and return its value. Raise KeyError if *key* not in maps[0]."""

    @overload
    def pop(self, key: _KT, default: _VT) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _T) -> _VT | _T: ...
    def copy(self) -> Self:
        """New ChainMap or subclass with a new copy of maps[0] and refs to maps[1:]"""
    __copy__ = copy
    # All arguments to `fromkeys` are passed to `dict.fromkeys` at runtime,
    # so the signature should be kept in line with `dict.fromkeys`.
    if sys.version_info >= (3, 13):
        @classmethod
        @overload
        def fromkeys(cls, iterable: Iterable[_T], /) -> ChainMap[_T, Any | None]:
            """Create a new ChainMap with keys from iterable and values set to value."""
    else:
        @classmethod
        @overload
        def fromkeys(cls, iterable: Iterable[_T]) -> ChainMap[_T, Any | None]:
            """Create a ChainMap with a single dict created from the iterable."""

    @classmethod
    @overload
    # Special-case None: the user probably wants to add non-None values later.
    def fromkeys(cls, iterable: Iterable[_T], value: None, /) -> ChainMap[_T, Any | None]:
        """Create a new ChainMap with keys from iterable and values set to value."""

    @classmethod
    @overload
    def fromkeys(cls, iterable: Iterable[_T], value: _S, /) -> ChainMap[_T, _S]: ...
    @overload
    def __or__(self, other: Mapping[_KT, _VT]) -> Self: ...
    @overload
    def __or__(self, other: Mapping[_T1, _T2]) -> ChainMap[_KT | _T1, _VT | _T2]: ...
    @overload
    def __ror__(self, other: Mapping[_KT, _VT]) -> Self: ...
    @overload
    def __ror__(self, other: Mapping[_T1, _T2]) -> ChainMap[_KT | _T1, _VT | _T2]: ...
    # ChainMap.__ior__ should be kept roughly in line with MutableMapping.update()
    @overload  # type: ignore[misc]
    def __ior__(self, other: SupportsKeysAndGetItem[_KT, _VT]) -> Self: ...
    @overload
    def __ior__(self, other: Iterable[tuple[_KT, _VT]]) -> Self: ...
