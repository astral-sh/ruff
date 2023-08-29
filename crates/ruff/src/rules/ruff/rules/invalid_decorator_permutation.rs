use ast::call_path::CallPath;
use itertools::Itertools;
use once_cell::sync::Lazy;
use ruff_python_semantic::SemanticModel;
use rustc_hash::{FxHashMap, FxHashSet};
use std::convert::From;
use std::option::Option;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Decorator, Expr};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Ensures that function decorators are in the proper order. This rule currently only supports
/// the following built-in decorators:
/// 1. `@abc.abstractmethod`
/// 1. `@classmethod`
/// 1. `@contextlib.asynccontextmanager`
/// 1. `@contextlib.contextmanager`
/// 1. `@functools.cache`
/// 1. `@functools.cached_property`
/// 1. `@functools.lru_cache`
/// 1. `@functools.wraps`
/// 1. `@property`
/// 1. `@property.setter`
///
/// ## Why is this bad?
/// If function decorators are improperly ordered, builds will fail.
///
/// ## Example
/// ```python
/// from abc import abstractmethod
/// class Foo:
///     @abstractmethod
///     @property
///     def foo(self):
///         ...
/// ```
/// In this case, the `abstractmethod` decorator requires that the object it wraps has a writable
/// `__isabstractmethod__` attribute. The `property` object's `__isabstractmethod__` attribute is
/// *not* writable, so running this code will fail.
///
/// Use instead:
/// ```python
/// from abc import abstractmethod
/// class Foo:
///     @property
///     @abstractmethod
///     def foo(self):
///         ...
/// ```
/// `abstractmethod` no longer wraps `property`, so we avoid the error!
#[violation]
pub(crate) struct InvalidDecoratorPermutation {
    dec1_name: String,
    dec2_name: String,
}

impl Violation for InvalidDecoratorPermutation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Invalid decorator permutation: {} cannot follow {}",
            self.dec2_name, self.dec1_name
        )
    }
}

/// RUF018
pub(crate) fn invalid_decorator_permutation(checker: &mut Checker, decorators: &Vec<Decorator>) {
    let Some(OrderedDecorators {
        preceding: dec1,
        following: dec2,
    }) = invalid_decorators(decorators, checker)
    else {
        return;
    };

    let Some(name1) = get_decorator_name(dec1) else {
        return;
    };
    let Some(name2) = get_decorator_name(dec2) else {
        return;
    };

    let diagnostic = Diagnostic::new(
        InvalidDecoratorPermutation {
            dec1_name: name1.to_string(),
            dec2_name: name2.to_string(),
        },
        TextRange::new(dec1.range.start(), dec2.range.end()),
    );

    checker.diagnostics.push(diagnostic);
}

/// Check a list of decorators to ensure that the permutation is valid. If the permutation is
/// valid, this function will return `None`. This function will short-circuit; it will only pick up
/// the first pair of incorrectly-ordered decorators.
fn invalid_decorators<'a>(
    decorators: &'a [Decorator],
    checker: &mut Checker,
) -> Option<OrderedDecorators<'a>> {
    if decorators.is_empty() {
        return None;
    };

    for (curr_decorator, next_decorator) in decorators.iter().tuple_windows() {
        let Some(curr) = BuiltinDecorator::try_from_decorator(curr_decorator, checker.semantic())
        else {
            continue;
        };
        let Some(next) = BuiltinDecorator::try_from_decorator(next_decorator, checker.semantic())
        else {
            continue;
        };

        if next.cannot_follow(&curr) {
            return Some(OrderedDecorators {
                preceding: curr_decorator,
                following: next_decorator,
            });
        }
    }

    None
}

struct OrderedDecorators<'a> {
    preceding: &'a Decorator,
    following: &'a Decorator,
}

fn get_decorator_name(decorator: &Decorator) -> Option<&str> {
    match &decorator.expression {
        Expr::Name(ast::ExprName { id, .. }) => Some(id),
        _ => None,
    }
}

/// Singleton map containing the relationships between decorators supported by RUF018. We use
/// `CAN_NOT_FOLLOW` to make it easier to support future decorators in the future. It is of the
/// form: { key_variant: { variants that key_variant cannot follow } }
static CAN_NOT_FOLLOW_MAP: Lazy<FxHashMap<BuiltinDecorator, FxHashSet<&BuiltinDecorator>>> =
    Lazy::new(|| {
        FxHashMap::from_iter([
            (
                BuiltinDecorator::AbstractMethod,
                FxHashSet::from_iter([&BuiltinDecorator::FunctoolsCachedProperty]),
            ),
            (
                BuiltinDecorator::AsyncContextManager,
                FxHashSet::from_iter([]),
            ),
            (
                BuiltinDecorator::ClassMethod,
                FxHashSet::from_iter([&BuiltinDecorator::FunctoolsLruCache]),
            ),
            (BuiltinDecorator::ContextManager, FxHashSet::from_iter([])),
            (BuiltinDecorator::FunctoolsCache, FxHashSet::from_iter([])),
            (
                BuiltinDecorator::FunctoolsCachedProperty,
                FxHashSet::from_iter([
                    &BuiltinDecorator::ClassMethod,
                    &BuiltinDecorator::FunctoolsCache,
                    &BuiltinDecorator::FunctoolsLruCache,
                ]),
            ),
            (
                BuiltinDecorator::FunctoolsLruCache,
                FxHashSet::from_iter([]),
            ),
            (
                BuiltinDecorator::FunctoolsWraps,
                FxHashSet::from_iter([&BuiltinDecorator::ClassMethod]),
            ),
            (
                // Not callable
                BuiltinDecorator::Property,
                FxHashSet::from_iter([
                    &BuiltinDecorator::AbstractMethod,
                    &BuiltinDecorator::FunctoolsCache,
                    &BuiltinDecorator::FunctoolsLruCache,
                ]),
            ),
            (BuiltinDecorator::PropertySetter, FxHashSet::from_iter([])),
            (
                BuiltinDecorator::StaticMethod,
                FxHashSet::from_iter([
                    &BuiltinDecorator::AbstractMethod,
                    &BuiltinDecorator::ContextManager,
                ]),
            ),
        ])
    });

#[derive(Debug, Eq, Hash, PartialEq)]
enum BuiltinDecorator {
    AbstractMethod,
    AsyncContextManager,
    ClassMethod,
    ContextManager,
    FunctoolsCache,
    FunctoolsCachedProperty,
    FunctoolsLruCache,
    FunctoolsWraps,
    Property,
    PropertySetter,
    StaticMethod,
}

impl BuiltinDecorator {
    /// Create a `BuiltinDecorator` from its fully-qualified name.
    fn try_from_call_path(qualified_name: CallPath) -> Option<Self> {
        match qualified_name.as_slice() {
            &["abc", "abstractmethod"] => Some(Self::AbstractMethod),
            &["contextlib", "asynccontextmanager"] => Some(Self::AsyncContextManager),
            &["", "classmethod"] => Some(Self::ClassMethod),
            &["contextlib", "contextmanager"] => Some(Self::ContextManager),
            &["functools", "cache"] => Some(Self::FunctoolsCache),
            &["functools", "cached_property"] => Some(Self::FunctoolsCachedProperty),
            &["functools", "lru_cache"] => Some(Self::FunctoolsLruCache),
            &["functools", "wraps"] => Some(Self::FunctoolsWraps),
            &["", "property"] => Some(Self::Property),
            &["property", "setter"] => Some(Self::PropertySetter),
            &["", "staticmethod"] => Some(Self::StaticMethod),
            _ => None,
        }
    }

    /// We can't implement TryFrom<Decorator> since `Decorator` isn't defined in this crate, so let's
    /// re-implement it. If the underlying `Decorator` isn't supported by RUF018, or the decorator
    /// is invalid (i.e., `@abstractmethod` is used but `from abc import abstractmethod` is
    /// absent), this will return `None`.
    fn try_from_decorator(decorator: &Decorator, semantic: &SemanticModel) -> Option<Self> {
        let Some(qualified_name) = semantic.resolve_call_path(&decorator.expression) else {
            return None;
        };

        Self::try_from_call_path(qualified_name)
    }

    /// If this `BuiltinDecorator` variant can follow another variant.
    fn cannot_follow(&self, other: &Self) -> bool {
        CAN_NOT_FOLLOW_MAP
            .get(self)
            .map_or(false, |entry| entry.contains(other))
    }
}
