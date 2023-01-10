use once_cell::sync::Lazy;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};

// See: https://pypi.org/project/typing-extensions/
pub static TYPING_EXTENSIONS: Lazy<FxHashSet<&'static str>> = Lazy::new(|| {
    FxHashSet::from_iter([
        "Annotated",
        "Any",
        "AsyncContextManager",
        "AsyncGenerator",
        "AsyncIterable",
        "AsyncIterator",
        "Awaitable",
        "ChainMap",
        "ClassVar",
        "Concatenate",
        "ContextManager",
        "Coroutine",
        "Counter",
        "DefaultDict",
        "Deque",
        "Final",
        "Literal",
        "LiteralString",
        "NamedTuple",
        "Never",
        "NewType",
        "NotRequired",
        "OrderedDict",
        "ParamSpec",
        "ParamSpecArgs",
        "ParamSpecKwargs",
        "Protocol",
        "Required",
        "Self",
        "TYPE_CHECKING",
        "Text",
        "Type",
        "TypeAlias",
        "TypeGuard",
        "TypeVar",
        "TypeVarTuple",
        "TypedDict",
        "Unpack",
        "assert_never",
        "assert_type",
        "clear_overloads",
        "final",
        "get_Type_hints",
        "get_args",
        "get_origin",
        "get_overloads",
        "is_typeddict",
        "overload",
        "override",
        "reveal_type",
        "runtime_checkable",
    ])
});

// See: https://docs.python.org/3/library/typing.html
static SUBSCRIPTS: Lazy<FxHashMap<&'static str, Vec<&'static str>>> = Lazy::new(|| {
    let mut subscripts: FxHashMap<&'static str, Vec<&'static str>> = FxHashMap::default();
    for (module, name) in [
        // builtins
        ("", "dict"),
        ("", "frozenset"),
        ("", "list"),
        ("", "set"),
        ("", "tuple"),
        ("", "type"),
        // `collections`
        ("collections", "ChainMap"),
        ("collections", "Counter"),
        ("collections", "OrderedDict"),
        ("collections", "defaultdict"),
        ("collections", "deque"),
        // `collections.abc`
        ("collections.abc", "AsyncGenerator"),
        ("collections.abc", "AsyncIterable"),
        ("collections.abc", "AsyncIterator"),
        ("collections.abc", "Awaitable"),
        ("collections.abc", "ByteString"),
        ("collections.abc", "Callable"),
        ("collections.abc", "Collection"),
        ("collections.abc", "Container"),
        ("collections.abc", "Coroutine"),
        ("collections.abc", "Generator"),
        ("collections.abc", "ItemsView"),
        ("collections.abc", "Iterable"),
        ("collections.abc", "Iterator"),
        ("collections.abc", "KeysView"),
        ("collections.abc", "Mapping"),
        ("collections.abc", "MappingView"),
        ("collections.abc", "MutableMapping"),
        ("collections.abc", "MutableSequence"),
        ("collections.abc", "MutableSet"),
        ("collections.abc", "Reversible"),
        ("collections.abc", "Sequence"),
        ("collections.abc", "Set"),
        ("collections.abc", "ValuesView"),
        // `contextlib`
        ("contextlib", "AbstractAsyncContextManager"),
        ("contextlib", "AbstractContextManager"),
        // `re`
        ("re", "Match"),
        ("re", "Pattern"),
        // `typing`
        ("typing", "AbstractSet"),
        ("typing", "AsyncContextManager"),
        ("typing", "AsyncGenerator"),
        ("typing", "AsyncIterator"),
        ("typing", "Awaitable"),
        ("typing", "BinaryIO"),
        ("typing", "ByteString"),
        ("typing", "Callable"),
        ("typing", "ChainMap"),
        ("typing", "ClassVar"),
        ("typing", "Collection"),
        ("typing", "Concatenate"),
        ("typing", "Container"),
        ("typing", "ContextManager"),
        ("typing", "Coroutine"),
        ("typing", "Counter"),
        ("typing", "DefaultDict"),
        ("typing", "Deque"),
        ("typing", "Dict"),
        ("typing", "Final"),
        ("typing", "FrozenSet"),
        ("typing", "Generator"),
        ("typing", "Generic"),
        ("typing", "IO"),
        ("typing", "ItemsView"),
        ("typing", "Iterable"),
        ("typing", "Iterator"),
        ("typing", "KeysView"),
        ("typing", "List"),
        ("typing", "Mapping"),
        ("typing", "Match"),
        ("typing", "MutableMapping"),
        ("typing", "MutableSequence"),
        ("typing", "MutableSet"),
        ("typing", "Optional"),
        ("typing", "OrderedDict"),
        ("typing", "Pattern"),
        ("typing", "Reversible"),
        ("typing", "Sequence"),
        ("typing", "Set"),
        ("typing", "TextIO"),
        ("typing", "Tuple"),
        ("typing", "Type"),
        ("typing", "TypeGuard"),
        ("typing", "Union"),
        ("typing", "Unpack"),
        ("typing", "ValuesView"),
        // `typing.io`
        ("typing.io", "BinaryIO"),
        ("typing.io", "IO"),
        ("typing.io", "TextIO"),
        // `typing.re`
        ("typing.re", "Match"),
        ("typing.re", "Pattern"),
        // `typing_extensions`
        ("typing_extensions", "AsyncContextManager"),
        ("typing_extensions", "AsyncGenerator"),
        ("typing_extensions", "AsyncIterable"),
        ("typing_extensions", "AsyncIterator"),
        ("typing_extensions", "Awaitable"),
        ("typing_extensions", "ChainMap"),
        ("typing_extensions", "ClassVar"),
        ("typing_extensions", "Concatenate"),
        ("typing_extensions", "ContextManager"),
        ("typing_extensions", "Coroutine"),
        ("typing_extensions", "Counter"),
        ("typing_extensions", "DefaultDict"),
        ("typing_extensions", "Deque"),
        ("typing_extensions", "Type"),
        // `weakref`
        ("weakref", "WeakKeyDictionary"),
        ("weakref", "WeakSet"),
        ("weakref", "WeakValueDictionary"),
    ] {
        subscripts.entry(name).or_default().push(module);
    }
    subscripts
});

// See: https://docs.python.org/3/library/typing.html
static PEP_593_SUBSCRIPTS: Lazy<FxHashMap<&'static str, Vec<&'static str>>> = Lazy::new(|| {
    let mut subscripts: FxHashMap<&'static str, Vec<&'static str>> = FxHashMap::default();
    for (module, name) in [
        // `typing`
        ("typing", "Annotated"),
        // `typing_extensions`
        ("typing_extensions", "Annotated"),
    ] {
        subscripts.entry(name).or_default().push(module);
    }
    subscripts
});

pub enum SubscriptKind {
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

pub fn match_annotated_subscript<'a, F>(
    expr: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
    typing_modules: impl Iterator<Item = &'a str>,
    is_builtin: F,
) -> Option<SubscriptKind>
where
    F: Fn(&str) -> bool,
{
    if !matches!(
        expr.node,
        ExprKind::Name { .. } | ExprKind::Attribute { .. }
    ) {
        return None;
    }
    let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
    if let Some(member) = call_path.last() {
        if let Some(modules) = SUBSCRIPTS.get(member) {
            for module in modules {
                if match_call_path(&call_path, module, member, from_imports)
                    && (!module.is_empty() || is_builtin(member))
                {
                    return Some(SubscriptKind::AnnotatedSubscript);
                }
            }
            for module in typing_modules {
                if match_call_path(&call_path, module, member, from_imports) {
                    return Some(SubscriptKind::AnnotatedSubscript);
                }
            }
        } else if let Some(modules) = PEP_593_SUBSCRIPTS.get(member) {
            for module in modules {
                if match_call_path(&call_path, module, member, from_imports)
                    && (!module.is_empty() || is_builtin(member))
                {
                    return Some(SubscriptKind::PEP593AnnotatedSubscript);
                }
            }
            for module in typing_modules {
                if match_call_path(&call_path, module, member, from_imports) {
                    return Some(SubscriptKind::PEP593AnnotatedSubscript);
                }
            }
        }
    }
    None
}

// See: https://peps.python.org/pep-0585/
const PEP_585_BUILTINS_ELIGIBLE: &[(&str, &str)] = &[
    ("typing", "Dict"),
    ("typing", "FrozenSet"),
    ("typing", "List"),
    ("typing", "Set"),
    ("typing", "Tuple"),
    ("typing", "Type"),
    ("typing_extensions", "Type"),
];

/// Returns `true` if `Expr` represents a reference to a typing object with a
/// PEP 585 built-in.
pub fn is_pep585_builtin(
    expr: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
    if !call_path.is_empty() {
        for (module, member) in PEP_585_BUILTINS_ELIGIBLE {
            if match_call_path(&call_path, module, member, from_imports) {
                return true;
            }
        }
    }
    false
}
