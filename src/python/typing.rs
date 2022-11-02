use std::collections::{BTreeMap, BTreeSet};

use once_cell::sync::Lazy;
use rustpython_ast::{Expr, ExprKind};

// See: https://docs.python.org/3/library/typing.html
static IMPORTED_SUBSCRIPTS: Lazy<BTreeMap<&'static str, BTreeSet<&'static str>>> =
    Lazy::new(|| {
        let mut import_map = BTreeMap::new();
        for (name, module) in [
            // `collections`
            ("ChainMap", "collections"),
            ("Counter", "collections"),
            ("OrderedDict", "collections"),
            ("defaultdict", "collections"),
            ("deque", "collections"),
            // `collections.abc`
            ("AsyncGenerator", "collections.abc"),
            ("AsyncIterable", "collections.abc"),
            ("AsyncIterator", "collections.abc"),
            ("Awaitable", "collections.abc"),
            ("ByteString", "collections.abc"),
            ("Callable", "collections.abc"),
            ("Collection", "collections.abc"),
            ("Container", "collections.abc"),
            ("Coroutine", "collections.abc"),
            ("Generator", "collections.abc"),
            ("ItemsView", "collections.abc"),
            ("Iterable", "collections.abc"),
            ("Iterator", "collections.abc"),
            ("KeysView", "collections.abc"),
            ("Mapping", "collections.abc"),
            ("MappingView", "collections.abc"),
            ("MutableMapping", "collections.abc"),
            ("MutableSequence", "collections.abc"),
            ("MutableSet", "collections.abc"),
            ("Reversible", "collections.abc"),
            ("Sequence", "collections.abc"),
            ("Set", "collections.abc"),
            ("ValuesView", "collections.abc"),
            // `contextlib`
            ("AbstractAsyncContextManager", "contextlib"),
            ("AbstractContextManager", "contextlib"),
            // `re`
            ("Match", "re"),
            ("Pattern", "re"),
            // `typing`
            ("AbstractSet", "typing"),
            ("Annotated", "typing"),
            ("AsyncContextManager", "typing"),
            ("AsyncContextManager", "typing"),
            ("AsyncGenerator", "typing"),
            ("AsyncIterable", "typing"),
            ("AsyncIterator", "typing"),
            ("Awaitable", "typing"),
            ("BinaryIO", "typing"),
            ("ByteString", "typing"),
            ("Callable", "typing"),
            ("ChainMap", "typing"),
            ("ClassVar", "typing"),
            ("Collection", "typing"),
            ("Concatenate", "typing"),
            ("Container", "typing"),
            ("ContextManager", "typing"),
            ("ContextManager", "typing"),
            ("Coroutine", "typing"),
            ("Counter", "typing"),
            ("DefaultDict", "typing"),
            ("Deque", "typing"),
            ("Dict", "typing"),
            ("Final", "typing"),
            ("FrozenSet", "typing"),
            ("Generator", "typing"),
            ("Generic", "typing"),
            ("IO", "typing"),
            ("ItemsView", "typing"),
            ("Iterable", "typing"),
            ("Iterator", "typing"),
            ("KeysView", "typing"),
            ("List", "typing"),
            ("Mapping", "typing"),
            ("Match", "typing"),
            ("MutableMapping", "typing"),
            ("MutableSequence", "typing"),
            ("MutableSet", "typing"),
            ("Optional", "typing"),
            ("OrderedDict", "typing"),
            ("Pattern", "typing"),
            ("Reversible", "typing"),
            ("Sequence", "typing"),
            ("Set", "typing"),
            ("TextIO", "typing"),
            ("Tuple", "typing"),
            ("Type", "typing"),
            ("Type", "typing"),
            ("TypeGuard", "typing"),
            ("Union", "typing"),
            ("Unpack", "typing"),
            ("ValuesView", "typing"),
            // `typing.io`
            ("BinaryIO", "typing.io"),
            ("IO", "typing.io"),
            ("TextIO", "typing.io"),
            // `typing.re`
            ("Match", "typing.re"),
            ("Pattern", "typing.re"),
            // `weakref`
            ("WeakKeyDictionary", "weakref"),
            ("WeakSet", "weakref"),
            ("WeakValueDictionary", "weakref"),
        ] {
            import_map
                .entry(name)
                .or_insert_with(BTreeSet::new)
                .insert(module);
        }
        import_map
    });

// These are all assumed to come from the `typing` module.
// See: https://peps.python.org/pep-0585/
static PEP_585_BUILTINS_ELIGIBLE: Lazy<BTreeSet<&'static str>> =
    Lazy::new(|| BTreeSet::from(["Dict", "FrozenSet", "List", "Set", "Tuple", "Type"]));

// These are all assumed to come from the `typing` module.
// See: https://peps.python.org/pep-0585/
static PEP_585_BUILTINS: Lazy<BTreeSet<&'static str>> =
    Lazy::new(|| BTreeSet::from(["dict", "frozenset", "list", "set", "tuple", "type"]));

fn is_pep593_annotated_subscript(name: &str) -> bool {
    name == "Annotated"
}

pub enum SubscriptKind {
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

pub fn match_annotated_subscript(
    expr: &Expr,
    imports: &BTreeMap<&str, BTreeSet<&str>>,
) -> Option<SubscriptKind> {
    match &expr.node {
        ExprKind::Attribute { attr, value, .. } => {
            if let ExprKind::Name { id, .. } = &value.node {
                // If `id` is `typing` and `attr` is `Union`, verify that `typing.Union` is an
                // annotated subscript.
                if IMPORTED_SUBSCRIPTS
                    .get(&attr.as_str())
                    .map(|imports| imports.contains(&id.as_str()))
                    .unwrap_or_default()
                {
                    return if is_pep593_annotated_subscript(attr) {
                        Some(SubscriptKind::PEP593AnnotatedSubscript)
                    } else {
                        Some(SubscriptKind::AnnotatedSubscript)
                    };
                }
            }
        }
        ExprKind::Name { id, .. } => {
            // Built-ins (no import necessary).
            if PEP_585_BUILTINS.contains(&id.as_str()) {
                return Some(SubscriptKind::AnnotatedSubscript);
            }

            // Verify that, e.g., `Union` is a reference to `typing.Union`.
            if let Some(modules) = IMPORTED_SUBSCRIPTS.get(&id.as_str()) {
                for module in modules {
                    if imports
                        .get(module)
                        .map(|imports| imports.contains(&id.as_str()))
                        .unwrap_or_default()
                    {
                        return if is_pep593_annotated_subscript(id) {
                            Some(SubscriptKind::PEP593AnnotatedSubscript)
                        } else {
                            Some(SubscriptKind::AnnotatedSubscript)
                        };
                    }
                }
            }
        }
        _ => {}
    }
    None
}

/// Returns `true` if `Expr` represents a reference to a typing object with a
/// PEP 585 built-in.
pub fn is_pep585_builtin(expr: &Expr, typing_imports: Option<&BTreeSet<&str>>) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, value, .. } => {
            if let ExprKind::Name { id, .. } = &value.node {
                id == "typing" && PEP_585_BUILTINS_ELIGIBLE.contains(&attr.as_str())
            } else {
                false
            }
        }
        ExprKind::Name { id, .. } => {
            typing_imports
                .map(|imports| imports.contains(&id.as_str()))
                .unwrap_or_default()
                && PEP_585_BUILTINS_ELIGIBLE.contains(&id.as_str())
        }
        _ => false,
    }
}
