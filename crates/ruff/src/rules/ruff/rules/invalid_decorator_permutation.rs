use itertools::Itertools;
use once_cell::sync::Lazy;
use ruff_text_size::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Decorator, Expr};

use crate::checkers::ast::Checker;

/// ## What it does
///
/// ## Why is this bad?
///
/// ## Example
/// ```python
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct InvalidDecoratorPermutation {
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

/// Map containing the initializations for decorators checked by RUF018.
static DECORATOR_PROPERTY_MAP: Lazy<FxHashMap<&str, &CheckableDecorator>> = Lazy::new(|| {
    // @abc.abstractmethod
    let abstract_method = CheckableDecorator::new(
        "abstractmethod",
        &[],
        &[DecoratorProperties::ReturnsWritableAbstractMethod(true)],
    );

    // @classmethod
    // @contextlib.asyncontextmanager
    // @contextlib.contextmanager
    // @functools.cache
    // @functools.cached_property
    // @functools.lru_cache
    // @functools.wrap
    // @property
    let property = CheckableDecorator::new(
        "property",
        &[DecoratorProperties::ReturnsWritableAbstractMethod(false)],
        &[],
    );

    // property.setter

    let decorators = [property, abstract_method]
        .into_iter()
        .map(|d| (d.name, &d));

    FxHashMap::from_iter(decorators)
});

/// RUF018
pub(crate) fn invalid_decorator_permutation(checker: &mut Checker, decorators: &Vec<Decorator>) {
    let Some((dec1, dec2)) = invalid_decorators(decorators) else {
        return;
    };

    let Some(dec1_name) = get_decorator_name(dec1) else {
        return;
    };
    let Some(dec2_name) = get_decorator_name(dec2) else {
        return;
    };

    let diagnostic = Diagnostic::new(
        InvalidDecoratorPermutation {
            dec1_name,
            dec2_name,
        },
        TextRange::new(dec1.range.start(), dec2.range.end()),
    );

    checker.diagnostics.push(diagnostic);
}

/// Check a list of decorators to ensure that the permutation is valid.  It returns an pair
/// comprised of decorators whose ordering is invalid. This function will short-circuit; it will
/// only pick up the first pair of incorrectly-ordered decorators.
fn invalid_decorators(decorators: &[Decorator]) -> Option<(&Decorator, &Decorator)> {
    for (curr, next) in decorators.iter().tuple_windows() {
        let curr_name = get_decorator_name(curr);
        let next_name = get_decorator_name(next);

        return Some((curr, next));
    }

    None
}

fn get_decorator_name(decorator: &Decorator) -> Option<String> {
    match &decorator.expression {
        Expr::Name(ast::ExprName { id, .. }) => Some(id.clone()),
        _ => None,
    }
}

/// A list of properties related to the decorators supported by RUF018. This is **INTENTIONALLY INCOMPLETE**
/// and should not be used outside of this rule.
#[derive(Eq, Hash, PartialEq)]
enum DecoratorProperties {
    ReturnsWritableAbstractMethod(bool), // __isabstractmethod__ of returned object is writable
}

struct CheckableDecorator<'a> {
    // The name of the decorator, e.g. `@<name>`.
    name: &'a str,
    // Properties of the object returned by the decorator function.
    returned_properties: FxHashSet<&'a DecoratorProperties>,
    // Properties that the decorator's parameters must possess.
    requires: FxHashSet<&'a DecoratorProperties>,
}

impl<'a> CheckableDecorator<'a> {
    fn new(
        name: &'a str,
        returned_properties: &'a [DecoratorProperties],
        requires: &'a [DecoratorProperties],
    ) -> Self {
        Self {
            name,
            returned_properties: FxHashSet::from_iter(returned_properties.iter()),
            requires: FxHashSet::from_iter(requires.iter()),
        }
    }

    fn can_wrap(&self, other: &CheckableDecorator) -> bool {
        self.requires.is_subset(other.returned_properties.as_ref())
    }
}

// property can't follow  abstractmethod
// contextmanager can't follow staticmethod
