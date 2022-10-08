use std::collections::BTreeSet;

use once_cell::sync::Lazy;

static ANNOTATED_SUBSCRIPTS: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
    BTreeSet::from([
        "AbstractAsyncContextManager",
        "AbstractContextManager",
        "AbstractSet",
        // "Annotated",
        "AsyncContextManager",
        "AsyncGenerator",
        "AsyncIterable",
        "AsyncIterator",
        "Awaitable",
        "BinaryIO",
        "BsdDbShelf",
        "ByteString",
        "Callable",
        "ChainMap",
        "ClassVar",
        "Collection",
        "Concatenate",
        "Container",
        "ContextManager",
        "Coroutine",
        "Counter",
        "Counter",
        "DbfilenameShelf",
        "DefaultDict",
        "Deque",
        "Dict",
        "Field",
        "Final",
        "FrozenSet",
        "Generator",
        "Iterator",
        "Generic",
        "IO",
        "ItemsView",
        "Iterable",
        "Iterator",
        "KeysView",
        "LifoQueue",
        "List",
        "Mapping",
        "MappingProxyType",
        "MappingView",
        "Match",
        "MutableMapping",
        "MutableSequence",
        "MutableSet",
        "Optional",
        "OrderedDict",
        "PathLike",
        "Pattern",
        "PriorityQueue",
        "Protocol",
        "Queue",
        "Reversible",
        "Sequence",
        "Set",
        "Shelf",
        "SimpleQueue",
        "TextIO",
        "Tuple",
        "Type",
        "TypeGuard",
        "Union",
        "ValuesView",
        "WeakKeyDictionary",
        "WeakMethod",
        "WeakSet",
        "WeakValueDictionary",
        "cached_property",
        "defaultdict",
        "deque",
        "dict",
        "frozenset",
        "list",
        "partialmethod",
        "set",
        "tuple",
        "type",
    ])
});

pub fn is_annotated_subscript(name: &str) -> bool {
    ANNOTATED_SUBSCRIPTS.contains(name)
}

pub fn is_pep593_annotated_subscript(name: &str) -> bool {
    name == "Annotated"
}

static PEP_585_BUILTINS: Lazy<BTreeSet<&'static str>> =
    Lazy::new(|| BTreeSet::from(["Dict", "FrozenSet", "List", "Set", "Tuple", "Type"]));

pub fn is_pep585_builtin(name: &str) -> bool {
    PEP_585_BUILTINS.contains(name)
}
