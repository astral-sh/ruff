# Since this module defines "overload" it is not recognized by Ruff as typing.overload
# ruff: noqa: F811
# TODO: The collections import is required, otherwise mypy crashes.
# https://github.com/python/mypy/issues/16744
import sys

if sys.version_info >= (3, 10):
    from types import UnionType

__all__ = [
    "AbstractSet",
    "Annotated",
    "Any",
    "AnyStr",
    "AsyncContextManager",
    "AsyncGenerator",
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "BinaryIO",
    "ByteString",
    "Callable",
    "ChainMap",
    "ClassVar",
    "Collection",
    "Container",
    "ContextManager",
    "Coroutine",
    "Counter",
    "DefaultDict",
    "Deque",
    "Dict",
    "Final",
    "ForwardRef",
    "FrozenSet",
    "Generator",
    "Generic",
    "Hashable",
    "IO",
    "ItemsView",
    "Iterable",
    "Iterator",
    "KeysView",
    "List",
    "Literal",
    "Mapping",
    "MappingView",
    "Match",
    "MutableMapping",
    "MutableSequence",
    "MutableSet",
    "NamedTuple",
    "NewType",
    "NoReturn",
    "Optional",
    "OrderedDict",
    "Pattern",
    "Protocol",
    "Reversible",
    "Sequence",
    "Set",
    "Sized",
    "SupportsAbs",
    "SupportsBytes",
    "SupportsComplex",
    "SupportsFloat",
    "SupportsIndex",
    "SupportsInt",
    "SupportsRound",
    "Text",
    "TextIO",
    "Tuple",
    "Type",
    "TypeVar",
    "TypedDict",
    "Union",
    "ValuesView",
    "TYPE_CHECKING",
    "cast",
    "final",
    "get_args",
    "get_origin",
    "get_type_hints",
    "no_type_check",
    "no_type_check_decorator",
    "overload",
    "runtime_checkable",
]

if sys.version_info >= (3, 10):
    __all__ += ["Concatenate", "ParamSpec", "ParamSpecArgs", "ParamSpecKwargs", "TypeAlias", "TypeGuard", "is_typeddict"]

if sys.version_info >= (3, 11):
    __all__ += [
        "LiteralString",
        "Never",
        "NotRequired",
        "Required",
        "Self",
        "TypeVarTuple",
        "Unpack",
        "assert_never",
        "assert_type",
        "clear_overloads",
        "dataclass_transform",
        "get_overloads",
        "reveal_type",
    ]

if sys.version_info >= (3, 12):
    __all__ += ["TypeAliasType", "override"]

if sys.version_info >= (3, 13):
    __all__ += ["get_protocol_members", "is_protocol", "NoDefault", "TypeIs", "ReadOnly"]

class Any: ...
class _Final: ...
