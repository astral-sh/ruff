use std::fmt;

use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Decorator;
use ruff_python_ast::helpers::map_callable;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for incorrect ordering of decorators on functions and methods.
///
/// ## Why is this bad?
/// Decorators are applied bottom-up. Certain decorator combinations cause
/// runtime errors or silent wrong behavior when placed in the wrong order.
/// For example, `@abstractmethod` must be the innermost (bottom) decorator
/// when combined with `@property`, `@classmethod`, or `@staticmethod`.
///
/// ## Example
/// ```python
/// from abc import abstractmethod
///
///
/// class Foo:
///     @abstractmethod
///     @property
///     def bar(self): ...
/// ```
///
/// Use instead:
/// ```python
/// from abc import abstractmethod
///
///
/// class Foo:
///     @property
///     @abstractmethod
///     def bar(self): ...
/// ```
///
/// ## References
/// - [Python documentation: `abc.abstractmethod`](https://docs.python.org/3/library/abc.html#abc.abstractmethod)
/// - [Python documentation: `contextlib.contextmanager`](https://docs.python.org/3/library/contextlib.html#contextlib.contextmanager)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct IncorrectDecoratorOrder {
    outer_decorator: KnownDecorator,
    inner_decorator: KnownDecorator,
}

impl Violation for IncorrectDecoratorOrder {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IncorrectDecoratorOrder {
            outer_decorator,
            inner_decorator,
        } = self;
        format!("`@{outer_decorator}` should be placed after `@{inner_decorator}`")
    }
}

/// RUF071
pub(crate) fn incorrect_decorator_order(checker: &Checker, decorator_list: &[Decorator]) {
    for (outer_decorator, inner_decorator) in decorator_list.iter().tuple_windows() {
        let (Some(outer), Some(inner)) = (
            classify_decorator(checker, outer_decorator),
            classify_decorator(checker, inner_decorator),
        ) else {
            continue;
        };
        if is_incorrect_order(outer, inner) {
            checker.report_diagnostic(
                IncorrectDecoratorOrder {
                    outer_decorator: outer,
                    inner_decorator: inner,
                },
                outer_decorator.range(),
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KnownDecorator {
    AbstractMethod,
    Property,
    ClassMethod,
    StaticMethod,
    ContextManager,
    AsyncContextManager,
    FunctoolsCache,
    FunctoolsCachedProperty,
    FunctoolsLruCache,
}

impl fmt::Display for KnownDecorator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AbstractMethod => "abstractmethod",
            Self::Property => "property",
            Self::ClassMethod => "classmethod",
            Self::StaticMethod => "staticmethod",
            Self::ContextManager => "contextmanager",
            Self::AsyncContextManager => "asynccontextmanager",
            Self::FunctoolsCache => "functools.cache",
            Self::FunctoolsCachedProperty => "functools.cached_property",
            Self::FunctoolsLruCache => "functools.lru_cache",
        })
    }
}

fn classify_decorator(checker: &Checker, decorator: &Decorator) -> Option<KnownDecorator> {
    let qualified_name = checker
        .semantic()
        .resolve_qualified_name(map_callable(&decorator.expression))?;
    match qualified_name.segments() {
        ["abc", "abstractmethod"] => Some(KnownDecorator::AbstractMethod),
        ["" | "builtins", "property"] | ["abc", "abstractproperty"] => {
            Some(KnownDecorator::Property)
        }
        ["" | "builtins", "classmethod"] | ["abc", "abstractclassmethod"] => {
            Some(KnownDecorator::ClassMethod)
        }
        ["" | "builtins", "staticmethod"] | ["abc", "abstractstaticmethod"] => {
            Some(KnownDecorator::StaticMethod)
        }
        ["contextlib", "contextmanager"] => Some(KnownDecorator::ContextManager),
        ["contextlib", "asynccontextmanager"] => Some(KnownDecorator::AsyncContextManager),
        ["functools", "cache"] => Some(KnownDecorator::FunctoolsCache),
        ["functools", "cached_property"] => Some(KnownDecorator::FunctoolsCachedProperty),
        ["functools", "lru_cache"] => Some(KnownDecorator::FunctoolsLruCache),
        _ => None,
    }
}

/// Returns `true` if `outer` above `inner` is a known-bad ordering.
fn is_incorrect_order(outer: KnownDecorator, inner: KnownDecorator) -> bool {
    matches!(
        (outer, inner),
        // @abstractmethod must be innermost when combined with descriptors
        (KnownDecorator::AbstractMethod, KnownDecorator::Property | KnownDecorator::ClassMethod | KnownDecorator::StaticMethod)
        // @contextmanager / @asynccontextmanager must wrap the raw function,
        // not a classmethod/staticmethod descriptor
        | (KnownDecorator::ContextManager | KnownDecorator::AsyncContextManager, KnownDecorator::StaticMethod | KnownDecorator::ClassMethod)
        // Caching decorators must not wrap descriptors
        | (KnownDecorator::FunctoolsCache | KnownDecorator::FunctoolsLruCache, KnownDecorator::Property | KnownDecorator::ClassMethod | KnownDecorator::FunctoolsCachedProperty)
        // @classmethod conflicts with @cached_property
        | (KnownDecorator::ClassMethod, KnownDecorator::FunctoolsCachedProperty)
    )
}
