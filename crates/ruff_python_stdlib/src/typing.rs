/// Returns `true` if a call path is a generic from the Python standard library (e.g. `list`, which
/// can be used as `list[int]`).
///
/// See: <https://docs.python.org/3/library/typing.html>
pub fn is_standard_library_generic(qualified_name: &[&str]) -> bool {
    matches!(
        qualified_name,
        [
            "" | "builtins",
            "dict" | "frozenset" | "list" | "set" | "tuple" | "type"
        ] | [
            "collections" | "typing" | "typing_extensions",
            "ChainMap" | "Counter"
        ] | ["collections" | "typing", "OrderedDict"]
            | ["collections", "defaultdict" | "deque"]
            | [
                "collections",
                "abc",
                "AsyncGenerator"
                    | "AsyncIterable"
                    | "AsyncIterator"
                    | "Awaitable"
                    | "ByteString"
                    | "Callable"
                    | "Collection"
                    | "Container"
                    | "Coroutine"
                    | "Generator"
                    | "ItemsView"
                    | "Iterable"
                    | "Iterator"
                    | "KeysView"
                    | "Mapping"
                    | "MappingView"
                    | "MutableMapping"
                    | "MutableSequence"
                    | "MutableSet"
                    | "Reversible"
                    | "Sequence"
                    | "Set"
                    | "ValuesView"
            ]
            | [
                "contextlib",
                "AbstractAsyncContextManager" | "AbstractContextManager"
            ]
            | ["re" | "typing", "Match" | "Pattern"]
            | [
                "typing",
                "AbstractSet"
                    | "AsyncContextManager"
                    | "AsyncGenerator"
                    | "AsyncIterator"
                    | "Awaitable"
                    | "BinaryIO"
                    | "ByteString"
                    | "Callable"
                    | "ClassVar"
                    | "Collection"
                    | "Concatenate"
                    | "Container"
                    | "ContextManager"
                    | "Coroutine"
                    | "DefaultDict"
                    | "Deque"
                    | "Dict"
                    | "Final"
                    | "FrozenSet"
                    | "Generator"
                    | "Generic"
                    | "IO"
                    | "ItemsView"
                    | "Iterable"
                    | "Iterator"
                    | "KeysView"
                    | "List"
                    | "Mapping"
                    | "MutableMapping"
                    | "MutableSequence"
                    | "MutableSet"
                    | "Optional"
                    | "Reversible"
                    | "Sequence"
                    | "Set"
                    | "TextIO"
                    | "Tuple"
                    | "Type"
                    | "TypeGuard"
                    | "Union"
                    | "Unpack"
                    | "ValuesView"
            ]
            | ["typing", "io", "BinaryIO" | "IO" | "TextIO"]
            | ["typing", "re", "Match" | "Pattern"]
            | [
                "typing_extensions",
                "AsyncContextManager"
                    | "AsyncGenerator"
                    | "AsyncIterable"
                    | "AsyncIterator"
                    | "Awaitable"
                    | "ClassVar"
                    | "Concatenate"
                    | "ContextManager"
                    | "Coroutine"
                    | "DefaultDict"
                    | "Deque"
                    | "Type"
            ]
            | [
                "weakref",
                "WeakKeyDictionary" | "WeakSet" | "WeakValueDictionary"
            ]
    )
}

/// Returns `true` if a call path is a [PEP 593] generic (e.g. `Annotated`).
///
/// See: <https://docs.python.org/3/library/typing.html>
///
/// [PEP 593]: https://peps.python.org/pep-0593/
pub fn is_pep_593_generic_type(qualified_name: &[&str]) -> bool {
    matches!(
        qualified_name,
        ["typing" | "typing_extensions", "Annotated"]
    )
}

/// Returns `true` if a call path is `Literal`.
pub fn is_standard_library_literal(qualified_name: &[&str]) -> bool {
    matches!(qualified_name, ["typing" | "typing_extensions", "Literal"])
}

/// Returns `true` if a name matches that of a generic from the Python standard library (e.g.
/// `list` or `Set`).
///
/// See: <https://docs.python.org/3/library/typing.html>
pub fn is_standard_library_generic_member(member: &str) -> bool {
    // Constructed by taking every pattern from `is_standard_library_generic`, removing all but
    // the last element in each pattern, and de-duplicating the values.
    matches!(
        member,
        "dict"
            | "AbstractAsyncContextManager"
            | "AbstractContextManager"
            | "AbstractSet"
            | "AsyncContextManager"
            | "AsyncGenerator"
            | "AsyncIterable"
            | "AsyncIterator"
            | "Awaitable"
            | "BinaryIO"
            | "ByteString"
            | "Callable"
            | "ChainMap"
            | "ClassVar"
            | "Collection"
            | "Concatenate"
            | "Container"
            | "ContextManager"
            | "Coroutine"
            | "Counter"
            | "DefaultDict"
            | "Deque"
            | "Dict"
            | "Final"
            | "FrozenSet"
            | "Generator"
            | "Generic"
            | "IO"
            | "ItemsView"
            | "Iterable"
            | "Iterator"
            | "KeysView"
            | "List"
            | "Mapping"
            | "MappingView"
            | "Match"
            | "MutableMapping"
            | "MutableSequence"
            | "MutableSet"
            | "Optional"
            | "OrderedDict"
            | "Pattern"
            | "Reversible"
            | "Sequence"
            | "Set"
            | "TextIO"
            | "Tuple"
            | "Type"
            | "TypeGuard"
            | "Union"
            | "Unpack"
            | "ValuesView"
            | "WeakKeyDictionary"
            | "WeakSet"
            | "WeakValueDictionary"
            | "defaultdict"
            | "deque"
            | "frozenset"
            | "list"
            | "set"
            | "tuple"
            | "type"
    )
}

/// Returns `true` if a name matches that of a generic from [PEP 593] (e.g. `Annotated`).
///
/// See: <https://docs.python.org/3/library/typing.html>
///
/// [PEP 593]: https://peps.python.org/pep-0593/
pub fn is_pep_593_generic_member(member: &str) -> bool {
    // Constructed by taking every pattern from `is_pep_593_generic`, removing all but
    // the last element in each pattern, and de-duplicating the values.
    matches!(member, "Annotated")
}

/// Returns `true` if a name matches that of the `Literal` generic.
pub fn is_literal_member(member: &str) -> bool {
    matches!(member, "Literal")
}

/// Returns `true` if a call path represents that of an immutable, non-generic type from the Python
/// standard library (e.g. `int` or `str`).
pub fn is_immutable_non_generic_type(qualified_name: &[&str]) -> bool {
    matches!(
        qualified_name,
        ["collections", "abc", "Sized"]
            | ["typing", "LiteralString" | "Sized"]
            | [
                "",
                "bool"
                    | "bytes"
                    | "complex"
                    | "float"
                    | "frozenset"
                    | "int"
                    | "object"
                    | "range"
                    | "str"
            ]
    )
}

/// Returns `true` if a call path represents that of an immutable, generic type from the Python
/// standard library (e.g. `tuple`).
pub fn is_immutable_generic_type(qualified_name: &[&str]) -> bool {
    matches!(
        qualified_name,
        ["" | "builtins", "tuple"]
            | [
                "collections",
                "abc",
                "ByteString"
                    | "Collection"
                    | "Container"
                    | "Iterable"
                    | "Mapping"
                    | "Reversible"
                    | "Sequence"
                    | "Set"
            ]
            | [
                "typing",
                "AbstractSet"
                    | "ByteString"
                    | "Callable"
                    | "Collection"
                    | "Container"
                    | "FrozenSet"
                    | "Iterable"
                    | "Literal"
                    | "Mapping"
                    | "Never"
                    | "NoReturn"
                    | "Reversible"
                    | "Sequence"
                    | "Tuple"
            ]
    )
}

/// Returns `true` if a call path represents a function from the Python standard library that
/// returns a mutable value (e.g., `dict`).
pub fn is_mutable_return_type(qualified_name: &[&str]) -> bool {
    matches!(
        qualified_name,
        ["" | "builtins", "dict" | "list" | "set"]
            | [
                "collections",
                "Counter" | "OrderedDict" | "defaultdict" | "deque"
            ]
    )
}

/// Returns `true` if a call path represents a function from the Python standard library that
/// returns a immutable value (e.g., `bool`).
pub fn is_immutable_return_type(qualified_name: &[&str]) -> bool {
    matches!(
        qualified_name,
        [
            "datetime",
            "date" | "datetime" | "time" | "timedelta" | "timezone" | "tzinfo"
        ] | ["decimal", "Decimal"]
            | ["fractions", "Fraction"]
            | ["operator", "attrgetter" | "itemgetter" | "methodcaller"]
            | ["pathlib", "Path"]
            | ["types", "MappingProxyType"]
            | ["re", "compile"]
            | [
                "",
                "bool" | "bytes" | "complex" | "float" | "frozenset" | "int" | "str" | "tuple"
            ]
    )
}

type ModuleMember = (&'static str, &'static str);

/// Given a typing member, returns the module and member name for a generic from the Python standard
/// library (e.g., `list` for `typing.List`), if such a generic was introduced by [PEP 585].
///
/// [PEP 585]: https://peps.python.org/pep-0585/
pub fn as_pep_585_generic(call_path: &[&str]) -> Option<ModuleMember> {
    match call_path {
        // Builtins
        ["typing" | "typing_extensions", "Tuple"] => Some(("", "tuple")),
        ["typing" | "typing_extensions", "List"] => Some(("", "list")),
        ["typing" | "typing_extensions", "Dict"] => Some(("", "dict")),
        ["typing" | "typing_extensions", "Set"] => Some(("", "set")),
        ["typing" | "typing_extensions", "FrozenSet"] => Some(("", "frozenset")),
        ["typing" | "typing_extensions", "Type"] => Some(("", "type")),

        // collections
        ["typing" | "typing_extensions", "Deque"] => Some(("collections", "deque")),
        ["typing" | "typing_extensions", "DefaultDict"] => Some(("collections", "defaultdict")),
        ["typing" | "typing_extensions", "OrderedDict"] => Some(("collections", "OrderedDict")),
        ["typing" | "typing_extensions", "Counter"] => Some(("collections", "Counter")),
        ["typing" | "typing_extensions", "ChainMap"] => Some(("collections", "ChainMap")),

        // collections.abc
        ["typing" | "typing_extensions", "Awaitable"] => Some(("collections.abc", "Awaitable")),
        ["typing" | "typing_extensions", "Coroutine"] => Some(("collections.abc", "Coroutine")),
        ["typing" | "typing_extensions", "AsyncIterable"] => {
            Some(("collections.abc", "AsyncIterable"))
        }
        ["typing" | "typing_extensions", "AsyncGenerator"] => {
            Some(("collections.abc", "AsyncGenerator"))
        }
        ["typing" | "typing_extensions", "Iterable"] => Some(("collections.abc", "Iterable")),
        ["typing" | "typing_extensions", "Iterator"] => Some(("collections.abc", "Iterator")),
        ["typing" | "typing_extensions", "Generator"] => Some(("collections.abc", "Generator")),
        ["typing" | "typing_extensions", "Reversible"] => Some(("collections.abc", "Reversible")),
        ["typing" | "typing_extensions", "Container"] => Some(("collections.abc", "Container")),
        ["typing" | "typing_extensions", "Collection"] => Some(("collections.abc", "Collection")),
        ["typing" | "typing_extensions", "Callable"] => Some(("collections.abc", "Callable")),
        ["typing" | "typing_extensions", "AbstractSet"] => Some(("collections.abc", "Set")),
        ["typing" | "typing_extensions", "MutableSet"] => Some(("collections.abc", "MutableSet")),
        ["typing" | "typing_extensions", "Mapping"] => Some(("collections.abc", "Mapping")),
        ["typing" | "typing_extensions", "MutableMapping"] => {
            Some(("collections.abc", "MutableMapping"))
        }
        ["typing" | "typing_extensions", "Sequence"] => Some(("collections.abc", "Sequence")),
        ["typing" | "typing_extensions", "MutableSequence"] => {
            Some(("collections.abc", "MutableSequence"))
        }
        ["typing" | "typing_extensions", "ByteString"] => Some(("collections.abc", "ByteString")),
        ["typing" | "typing_extensions", "MappingView"] => Some(("collections.abc", "MappingView")),
        ["typing" | "typing_extensions", "KeysView"] => Some(("collections.abc", "KeysView")),
        ["typing" | "typing_extensions", "ItemsView"] => Some(("collections.abc", "ItemsView")),
        ["typing" | "typing_extensions", "ValuesView"] => Some(("collections.abc", "ValuesView")),

        // contextlib
        ["typing" | "typing_extensions", "ContextManager"] => {
            Some(("contextlib", "AbstractContextManager"))
        }
        ["typing" | "typing_extensions", "AsyncContextManager"] => {
            Some(("contextlib", "AbstractAsyncContextManager"))
        }

        // re
        ["typing" | "typing_extensions", "Pattern"] => Some(("re", "Pattern")),
        ["typing" | "typing_extensions", "Match"] => Some(("re", "Match")),
        ["typing" | "typing_extensions", "re", "Pattern"] => Some(("re", "Pattern")),
        ["typing" | "typing_extensions", "re", "Match"] => Some(("re", "Match")),

        _ => None,
    }
}

/// Given a typing member, returns `true` if a generic equivalent exists in the Python standard
/// library (e.g., `list` for `typing.List`), as introduced by [PEP 585].
///
/// [PEP 585]: https://peps.python.org/pep-0585/
pub fn has_pep_585_generic(module: &str, member: &str) -> bool {
    // Constructed by taking every pattern from `as_pep_585_generic`, removing all but
    // the last element in each pattern, and de-duplicating the values.
    matches!(
        (module, member),
        ("", "tuple" | "list" | "dict" | "set" | "frozenset" | "type")
            | (
                "collections",
                "deque" | "defaultdict" | "OrderedDict" | "Counter" | "ChainMap"
            )
            | (
                "collections.abc",
                "Awaitable"
                    | "Coroutine"
                    | "Iterable"
                    | "Iterator"
                    | "Generator"
                    | "Reversible"
                    | "Container"
                    | "Collection"
                    | "Callable"
                    | "Set"
                    | "MutableSet"
                    | "Mapping"
                    | "MutableMapping"
                    | "Sequence"
                    | "MutableSequence"
                    | "ByteString"
                    | "MappingView"
                    | "KeysView"
                    | "ItemsView"
                    | "ValuesView"
            )
            | (
                "contextlib",
                "AbstractContextManager" | "AbstractAsyncContextManager"
            )
            | ("re", "Pattern" | "Match")
    )
}

/// Returns the expected return type for a magic method.
///
/// See: <https://github.com/JelleZijlstra/autotyping/blob/0adba5ba0eee33c1de4ad9d0c79acfd737321dd9/autotyping/autotyping.py#L69-L91>
pub fn simple_magic_return_type(method: &str) -> Option<&'static str> {
    match method {
        "__str__" | "__repr__" | "__format__" => Some("str"),
        "__bytes__" => Some("bytes"),
        "__len__" | "__length_hint__" | "__int__" | "__index__" => Some("int"),
        "__float__" => Some("float"),
        "__complex__" => Some("complex"),
        "__bool__" | "__contains__" | "__instancecheck__" | "__subclasscheck__" => Some("bool"),
        "__init__" | "__del__" | "__setattr__" | "__delattr__" | "__setitem__" | "__delitem__"
        | "__set__" => Some("None"),
        _ => None,
    }
}
