use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use rustpython_ast::{Expr, ExprKind};

// TODO(charlie): Some of these are actually from `collections`.
// Review: https://peps.python.org/pep-0585/.
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
        "defaultdict",
        "deque",
        "dict",
        "frozenset",
        "list",
        "set",
        "tuple",
        "type",
    ])
});

static PEP_585_BUILTINS_ELIGIBLE: Lazy<BTreeSet<&'static str>> =
    Lazy::new(|| BTreeSet::from(["Dict", "FrozenSet", "List", "Set", "Tuple", "Type"]));

static PEP_585_BUILTINS: Lazy<BTreeSet<&'static str>> =
    Lazy::new(|| BTreeSet::from(["dict", "frozenset", "list", "set", "tuple", "type"]));

fn is_annotated_subscript(name: &str) -> bool {
    ANNOTATED_SUBSCRIPTS.contains(name)
}

fn is_pep593_annotated_subscript(name: &str) -> bool {
    name == "Annotated"
}

pub enum SubscriptKind {
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

pub fn match_annotated_subscript(
    expr: &Expr,
    imports: Option<&BTreeSet<&str>>,
) -> Option<SubscriptKind> {
    match &expr.node {
        ExprKind::Attribute { attr, value, .. } => {
            if let ExprKind::Name { id, .. } = &value.node {
                if id == "typing" {
                    if is_pep593_annotated_subscript(attr) {
                        return Some(SubscriptKind::PEP593AnnotatedSubscript);
                    } else if is_annotated_subscript(attr) {
                        return Some(SubscriptKind::AnnotatedSubscript);
                    }
                }
            }
        }
        ExprKind::Name { id, .. } => {
            // Built-ins (no import necessary).
            if PEP_585_BUILTINS.contains(&id.as_str()) {
                return Some(SubscriptKind::AnnotatedSubscript);
            }

            if imports
                .map(|import| import.contains(&id.as_str()))
                .unwrap_or_default()
            {
                if is_pep593_annotated_subscript(id) {
                    return Some(SubscriptKind::PEP593AnnotatedSubscript);
                } else if is_annotated_subscript(id) {
                    return Some(SubscriptKind::AnnotatedSubscript);
                }
            }
        }
        _ => {}
    }
    None
}

/// Returns `true` if `Expr` represents a reference to a typing object with a PEP585 built-in.
pub fn is_pep585_builtin(expr: &Expr, imports: Option<&BTreeSet<&str>>) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, value, .. } => {
            if let ExprKind::Name { id, .. } = &value.node {
                id == "typing" && PEP_585_BUILTINS_ELIGIBLE.contains(&attr.as_str())
            } else {
                false
            }
        }
        ExprKind::Name { id, .. } => {
            imports
                .map(|import| import.contains(&id.as_str()))
                .unwrap_or_default()
                && PEP_585_BUILTINS_ELIGIBLE.contains(&id.as_str())
        }
        _ => false,
    }
}
