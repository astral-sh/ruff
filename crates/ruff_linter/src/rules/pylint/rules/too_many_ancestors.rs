use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes with too many parent classes.
///
/// By default, this rule allows up to 7 parent classes, as configured by
/// the [`lint.pylint.max-ancestors`] option.
///
/// ## Why is this bad?
/// Classes with many ancestors are harder to understand, use, and maintain.
///
/// Instead, consider hierarchically refactoring the class into separate classes.
///
/// ## Example
/// Assuming that `lint.pylint.max-ancestors` is set to 7:
///
/// ```python
/// class Animal:
///     ...
///
///
/// class BeakyAnimal(Animal):
///     ...
///
///
/// class FurryAnimal(Animal):
///     ...
///
///
/// class Swimmer(Animal):
///     ...
///
///
/// class EggLayer(Animal):
///     ...
///
///
/// class VenomousAnimal(Animal):
///     ...
///
///
/// class ProtectedSpecies(Animal):
///     ...
///
///
/// class BeaverTailedAnimal(Animal):
///     ...
///
///
/// class Vertebrate(Animal):
///     ...
///
///
/// class Platypus(  # [too-many-ancestors]
///     BeakyAnimal,
///     FurryAnimal,
///     Swimmer,
///     EggLayer,
///     VenomousAnimal,
///     ProtectedSpecies,
///     BeaverTailedAnimal,
///     Vertebrate,
/// ):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// class Animal:
///     beaver_tailed: bool
///     can_swim: bool
///     has_beak: bool
///     has_fur: bool
///     has_vertebrae: bool
///     lays_egg: bool
///     protected_species: bool
///     venomous: bool
///
///
/// class Invertebrate(Animal):
///     has_vertebrae = False
///
///
/// class Vertebrate(Animal):
///     has_vertebrae = True
///
///
/// class Mammal(Vertebrate):
///     has_beak = False
///     has_fur = True
///     lays_egg = False
///     venomous = False
///
///
/// class Platypus(Mammal):
///     beaver_tailed = True
///     can_swim = True
///     has_beak = True
///     lays_egg = True
///     protected_species = True
///     venomous = True
/// ```
///
/// ## Options
/// - `lint.pylint.max-ancestors`
#[violation]
pub struct TooManyAncestors {
    n_ancestors: usize,
    max_ancestors: usize,
}

impl Violation for TooManyAncestors {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyAncestors {
            n_ancestors,
            max_ancestors,
        } = self;
        format!("Too many ancestors ({n_ancestors} > {max_ancestors})")
    }
}

/// R0901
pub(crate) fn too_many_ancestors(
    checker: &mut Checker,
    class_def: &ast::StmtClassDef,
    max_ancestors: usize,
) {
    let n_ancestors = class_def.bases().len();

    if n_ancestors > max_ancestors {
        checker.diagnostics.push(Diagnostic::new(
            TooManyAncestors {
                n_ancestors,
                max_ancestors,
            },
            class_def.range(),
        ));
    }
}
