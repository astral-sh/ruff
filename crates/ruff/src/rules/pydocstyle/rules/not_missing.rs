use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::cast;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility::{
    is_call, is_init, is_magic, is_new, is_overload, is_override, Visibility,
};
use ruff_python_semantic::{Definition, Member, MemberKind, Module, ModuleKind};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for public module definitions that are missing docstrings.
///
/// ## Why is this bad?
/// Public modules should have docstrings so that users can understand their
/// purpose and how to use them.
///
/// Generally, module docstrings should describe the purpose of the module and
/// summarise objects that are exported by the module. If the module is simple,
/// a one-line docstring may be sufficient.
///
/// _If the codebase has a standard format for module docstrings, follow that
/// format for consistency._
///
/// ## Example
/// ```python
/// class FasterThanLightError(ZeroDivisionError):
///     ...
///
///
/// def calculate_speed(distance: float, time: float) -> float:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// """Utility functions and classes for calculating speed.
///
/// This module provides:
///     - FasterThanLightError: exception when FTL speed is calculated;
///     - calculate_speed: calculate speed given distance and time.
/// """
///
///
/// class FasterThanLightError(ZeroDivisionError):
///     ...
///
///
/// def calculate_speed(distance: float, time: float) -> float:
///     ...
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct UndocumentedPublicModule;

impl Violation for UndocumentedPublicModule {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public module")
    }
}

/// ## What it does
/// Checks for public class definitions that are missing docstrings.
///
/// ## Why is this bad?
/// Public classes should have docstrings so that users can understand their
/// purpose and how to use them.
///
/// Generally, class docstrings should describe its behavior and list its public
/// attributes and methods. If the class is simple, a one-line docstring may be
/// sufficient.
///
/// _If the codebase has a standard format for class docstrings, follow that
/// format for consistency._
///
/// ## Example
/// ```python
/// class Player:
///     def __init__(self, name: str, points: int = 0) -> None:
///         self.name: str = name
///         self.points: int = points
///
///     def add_points(self, points: int) -> None:
///         self.points += points
/// ```
///
/// Use instead (with NumPy docstring format):
/// ```python
/// class Player:
///     """A player in the game.
///
///     Attributes
///     ----------
///     name : str
///         The name of the player.
///     points : int
///         The number of points the player has.
///
///     Methods
///     -------
///     add_points(points: int) -> None
///         Add points to the player's score.
///     """
///
///     def __init__(self, name: str, points: int = 0) -> None:
///         self.name: str = name
///         self.points: int = points
///
///     def add_points(self, points: int) -> None:
///         self.points += points
/// ```
///
/// Or, using Google docstring format:
/// ```python
/// class Player:
///     """A player in the game.
///     
///     Attributes:
///         name: The name of the player.
///         points: The number of points the player has.
///     """
///     
///     def __init__(self, name: str, points: int = 0) -> None:
///         self.name: str = name
///         self.points: int = points
///
///     def add_points(self, points: int) -> None:
///         self.points += points
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct UndocumentedPublicClass;

impl Violation for UndocumentedPublicClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public class")
    }
}

/// ## What it does
/// Checks for public method definitions that are missing docstrings.
///
/// ## Why is this bad?
/// Public methods should have docstrings so that users can understand their
/// purpose and how to use them.
///
/// Generally, method docstrings should describe its behavior, arguments, side
/// effects, exceptions, return values, and any other information that is
/// relevant to the user. If the method is simple, a one-line docstring may be
/// sufficient.
///
/// _If the codebase has a standard format for method docstrings, follow that
/// format for consistency._
///
/// ## Example
/// ```python
/// class Cat(Animal):
///     def greet(self, happy: bool = True):
///         if happy:
///             print("Meow!")
///         else:
///             raise ValueError("Tried to greet an unhappy cat.")
/// ```
///
/// Use instead (with NumPy docstring format):
/// ```python
/// class Cat(Animal):
///     def greet(self, happy: bool = True):
///         """Print a greeting from the cat.
///
///         Parameters
///         ----------
///         happy : bool, optional
///             Whether the cat is happy, is True by default.
///
///         Raises
///         ------
///         ValueError
///             If the cat is not happy.
///         """
///         if happy:
///             print("Meow!")
///         else:
///             raise ValueError("Tried to greet an unhappy cat.")
/// ```
///
/// Or, using Google docstring format:
/// ```python
/// class Cat(Animal):
///     def greet(self, happy: bool = True):
///         """Print a greeting from the cat.
///         
///         Args:
///             happy: Whether the cat is happy, is True by default.
///
///         Raises:
///             ValueError: If the cat is not happy.
///         """
///         if happy:
///             print("Meow!")
///         else:
///             raise ValueError("Tried to greet an unhappy cat.")
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct UndocumentedPublicMethod;

impl Violation for UndocumentedPublicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public method")
    }
}

/// ## What it does
/// Checks for public function definitions that are missing docstrings.
///
/// ## Why is this bad?
/// Public functions should have docstrings so that users can understand their
/// purpose and how to use them.
///
/// Generally, function docstrings should describe its behavior, arguments, side
/// effects, exceptions, return values, and any other information that is
/// relevant to the user. If the function is simple, a one-line docstring may
/// be sufficient.
///
/// _If the codebase has a standard format for function docstrings, follow that
/// format for consistency._
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead (using the NumPy docstring format):
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Parameters
///     ----------
///     distance : float
///         Distance traveled.
///     time : float
///         Time spent traveling.
///
///     Returns
///     -------
///     float
///         Speed as distance divided by time.
///
///     Raises
///     ------
///     FasterThanLightError
///         If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Or, using the Google docstring format:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///     
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Python Docstrings](https://google.github.io/styleguide/pyguide.html#s3.8-comments-and-docstrings)
#[violation]
pub struct UndocumentedPublicFunction;

impl Violation for UndocumentedPublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public function")
    }
}

/// ## What it does
/// Checks for public package definitions that are missing docstrings.
///
/// ## Why is this bad?
/// Public packages should have docstrings so that users can understand their
/// purpose and how to use them.
///
/// Generally, package docstrings should list the modules and subpackages that
/// are exported by the package. If the of the package is simple, a one-line
/// docstring may be sufficient.
///
/// _If the codebase has a standard format for package docstrings, follow that
/// format for consistency._
///
/// ## Example
/// ```python
/// __all__ = ["Player", "Game"]
/// ```
///
/// Use instead:
/// ```python
/// """Game and player management package.
///
/// This package provides classes for managing players and games.
/// """
///
/// __all__ = ["player", "game"]
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Python Docstrings](https://google.github.io/styleguide/pyguide.html#s3.8-comments-and-docstrings)
#[violation]
pub struct UndocumentedPublicPackage;

impl Violation for UndocumentedPublicPackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public package")
    }
}

/// ## What it does
/// Checks for magic method definitions that are missing docstrings.
///
/// ## Why is this bad?
/// Magic methods (methods with names that start and end with double
/// underscores) are used to implement operator overloading and other special
/// behavior. They should have docstrings so that users can understand their
/// purpose and how to use them.
///
/// Generally, magic method docstrings should describe its behavior, arguments,
/// side effects, exceptions, return values, and any other information that is
/// relevant to the user. If the method is simple, a one-line description may
/// be sufficient.
///
/// _If the codebase has a standard format for magic method docstrings, follow
/// that format for consistency._
///
/// ## Example
/// ```python
/// class Cat(Animal):
///     def __str__(self) -> str:
///         return f"Cat: {self.name}"
///
///
/// cat = Cat("Dusty")
/// print(cat)  # "Cat: Dusty"
/// ```
///
/// Use instead:
/// ```python
/// class Cat(Animal):
///     def __str__(self) -> str:
///         """Return a string representation of the cat."""
///         return f"Cat: {self.name}"
///
///
/// cat = Cat("Dusty")
/// print(cat)  # "Cat: Dusty"
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Python Docstrings](https://google.github.io/styleguide/pyguide.html#s3.8-comments-and-docstrings)
#[violation]
pub struct UndocumentedMagicMethod;

impl Violation for UndocumentedMagicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in magic method")
    }
}

/// ## What it does
/// Checks for public class definitions that are missing docstrings.
///
/// ## Why is this bad?
/// Public classes should have docstrings so that users can understand their
/// purpose and how to use them.
///
/// Nested classes do not inherit the docstring of their enclosing class, so
/// they should have their own docstrings.
///
/// _If the codebase has a standard format for nested class docstrings, follow
/// that format for consistency._
///
/// ## Example
/// ```python
/// class Foo:
///     """Class Foo."""
///
///     class Bar:
///         ...
///
///
/// bar = Foo.Bar()
/// bar.__doc__  # None
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     """Class Foo."""
///
///     class Bar:
///         """Class Bar."""
///
///
/// bar = Foo.Bar()
/// bar.__doc__  # "Class Bar."
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Python Docstrings](https://google.github.io/styleguide/pyguide.html#s3.8-comments-and-docstrings)
#[violation]
pub struct UndocumentedPublicNestedClass;

impl Violation for UndocumentedPublicNestedClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public nested class")
    }
}

/// ## What it does
/// Checks for public `__init__` method definitions that are missing
/// docstrings.
///
/// ## Why is this bad?
/// Public `__init__` methods are used to initialize objects. They should have
/// docstrings so that users can understand how to use them.
///
/// Generally, `__init__` docstrings should describe its behavior, arguments,
/// side effects, exceptions, and any other information that is relevant to the
/// user. If the method is simple, a one-line description may be sufficient.
///
/// _If the codebase has a standard format for `__init__` docstrings, follow
/// that format for consistency._
///
/// ## Example
/// ```python
/// class City:
///     def __init__(self, name: str, population: int) -> None:
///         self.name: str = name
///         self.population: int = population
/// ```
///
/// Use instead:
/// ```python
/// class City:
///     def __init__(self, name: str, population: int) -> None:
///         """Initialize a city with a name and population."""
///         self.name: str = name
///         self.population: int = population
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Style Python Docstrings](https://google.github.io/styleguide/pyguide.html#s3.8-comments-and-docstrings)
#[violation]
pub struct UndocumentedPublicInit;

impl Violation for UndocumentedPublicInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in `__init__`")
    }
}

/// D100, D101, D102, D103, D104, D105, D106, D107
pub(crate) fn not_missing(
    checker: &mut Checker,
    definition: &Definition,
    visibility: Visibility,
) -> bool {
    if visibility.is_private() {
        return true;
    }

    match definition {
        Definition::Module(Module {
            kind: ModuleKind::Module,
            ..
        }) => {
            if checker.enabled(Rule::UndocumentedPublicModule) {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicModule,
                    TextRange::default(),
                ));
            }
            false
        }
        Definition::Module(Module {
            kind: ModuleKind::Package,
            ..
        }) => {
            if checker.enabled(Rule::UndocumentedPublicPackage) {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicPackage,
                    TextRange::default(),
                ));
            }
            false
        }
        Definition::Member(Member {
            kind: MemberKind::Class,
            stmt,
            ..
        }) => {
            if checker.enabled(Rule::UndocumentedPublicClass) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(UndocumentedPublicClass, stmt.identifier()));
            }
            false
        }
        Definition::Member(Member {
            kind: MemberKind::NestedClass,
            stmt,
            ..
        }) => {
            if checker.enabled(Rule::UndocumentedPublicNestedClass) {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicNestedClass,
                    stmt.identifier(),
                ));
            }
            false
        }
        Definition::Member(Member {
            kind: MemberKind::Function | MemberKind::NestedFunction,
            stmt,
            ..
        }) => {
            if is_overload(cast::decorator_list(stmt), checker.semantic()) {
                true
            } else {
                if checker.enabled(Rule::UndocumentedPublicFunction) {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicFunction,
                        stmt.identifier(),
                    ));
                }
                false
            }
        }
        Definition::Member(Member {
            kind: MemberKind::Method,
            stmt,
            ..
        }) => {
            if is_overload(cast::decorator_list(stmt), checker.semantic())
                || is_override(cast::decorator_list(stmt), checker.semantic())
            {
                true
            } else if is_init(cast::name(stmt)) {
                if checker.enabled(Rule::UndocumentedPublicInit) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(UndocumentedPublicInit, stmt.identifier()));
                }
                true
            } else if is_new(cast::name(stmt)) || is_call(cast::name(stmt)) {
                if checker.enabled(Rule::UndocumentedPublicMethod) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(UndocumentedPublicMethod, stmt.identifier()));
                }
                true
            } else if is_magic(cast::name(stmt)) {
                if checker.enabled(Rule::UndocumentedMagicMethod) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(UndocumentedMagicMethod, stmt.identifier()));
                }
                true
            } else {
                if checker.enabled(Rule::UndocumentedPublicMethod) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(UndocumentedPublicMethod, stmt.identifier()));
                }
                true
            }
        }
    }
}
