use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility::{
    is_call, is_init, is_magic, is_new, is_overload, is_override, Visibility,
};
use ruff_python_semantic::{Definition, Member, MemberKind, Module, ModuleKind};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for undocumented public module definitions.
///
/// ## Why is this bad?
/// Public modules should be documented via docstrings to outline their purpose
/// and contents.
///
/// Generally, module docstrings should describe the purpose of the module and
/// list the classes, exceptions, functions, and other objects that are exported
/// by the module, alongside a one-line summary of each.
///
/// If the module is a script, the docstring should be usable as its "usage"
/// message.
///
/// If the codebase adheres to a standard format for module docstrings, follow
/// that format for consistency.
///
/// ## Example
///
/// ```python
/// class FasterThanLightError(ZeroDivisionError): ...
///
///
/// def calculate_speed(distance: float, time: float) -> float: ...
/// ```
///
/// Use instead:
///
/// ```python
/// """Utility functions and classes for calculating speed.
///
/// This module provides:
/// - FasterThanLightError: exception when FTL speed is calculated;
/// - calculate_speed: calculate speed given distance and time.
/// """
///
///
/// class FasterThanLightError(ZeroDivisionError): ...
///
///
/// def calculate_speed(distance: float, time: float) -> float: ...
/// ```
///
/// ## Notebook behavior
/// This rule is ignored for Jupyter Notebooks.
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
/// Checks for undocumented public class definitions.
///
/// ## Why is this bad?
/// Public classes should be documented via docstrings to outline their purpose
/// and behavior.
///
/// Generally, a class docstring should describe the class's purpose and list
/// its public attributes and methods.
///
/// If the codebase adheres to a standard format for class docstrings, follow
/// that format for consistency.
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
/// Use instead (in the NumPy docstring format):
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
/// Or (in the Google docstring format):
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
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
/// Checks for undocumented public method definitions.
///
/// ## Why is this bad?
/// Public methods should be documented via docstrings to outline their purpose
/// and behavior.
///
/// Generally, a method docstring should describe the method's behavior,
/// arguments, side effects, exceptions, return values, and any other
/// information that may be relevant to the user.
///
/// If the codebase adheres to a standard format for method docstrings, follow
/// that format for consistency.
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
/// Use instead (in the NumPy docstring format):
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
/// Or (in the Google docstring format):
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
/// ## Options
/// - `lint.pydocstyle.ignore-decorators`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
/// Checks for undocumented public function definitions.
///
/// ## Why is this bad?
/// Public functions should be documented via docstrings to outline their
/// purpose and behavior.
///
/// Generally, a function docstring should describe the function's behavior,
/// arguments, side effects, exceptions, return values, and any other
/// information that may be relevant to the user.
///
/// If the codebase adheres to a standard format for function docstrings, follow
/// that format for consistency.
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
/// ```
///
/// ## Options
/// - `lint.pydocstyle.ignore-decorators`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
/// Checks for undocumented public package definitions.
///
/// ## Why is this bad?
/// Public packages should be documented via docstrings to outline their
/// purpose and contents.
///
/// Generally, package docstrings should list the modules and subpackages that
/// are exported by the package.
///
/// If the codebase adheres to a standard format for package docstrings, follow
/// that format for consistency.
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
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
/// Checks for undocumented magic method definitions.
///
/// ## Why is this bad?
/// Magic methods (methods with names that start and end with double
/// underscores) are used to implement operator overloading and other special
/// behavior. Such methods should be documented via docstrings to
/// outline their behavior.
///
/// Generally, magic method docstrings should describe the method's behavior,
/// arguments, side effects, exceptions, return values, and any other
/// information that may be relevant to the user.
///
/// If the codebase adheres to a standard format for method docstrings, follow
/// that format for consistency.
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
/// ## Options
/// - `lint.pydocstyle.ignore-decorators`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
/// Checks for undocumented public class definitions, for nested classes.
///
/// ## Why is this bad?
/// Public classes should be documented via docstrings to outline their
/// purpose and behavior.
///
/// Nested classes do not inherit the docstring of their enclosing class, so
/// they should have their own docstrings.
///
/// If the codebase adheres to a standard format for class docstrings, follow
/// that format for consistency.
///
/// ## Example
///
/// ```python
/// class Foo:
///     """Class Foo."""
///
///     class Bar: ...
///
///
/// bar = Foo.Bar()
/// bar.__doc__  # None
/// ```
///
/// Use instead:
///
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
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
/// Public `__init__` methods are used to initialize objects. `__init__`
/// methods should be documented via docstrings to describe the method's
/// behavior, arguments, side effects, exceptions, and any other information
/// that may be relevant to the user.
///
/// If the codebase adheres to a standard format for `__init__` method docstrings,
/// follow that format for consistency.
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
/// ## Options
/// - `lint.pydocstyle.ignore-decorators`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [PEP 287 – reStructuredText Docstring Format](https://peps.python.org/pep-0287/)
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
    if checker.source_type.is_stub() {
        return true;
    }

    if visibility.is_private() {
        return true;
    }

    match definition {
        Definition::Module(Module {
            kind: ModuleKind::Module,
            ..
        }) => {
            if checker.source_type.is_ipynb() {
                return true;
            }
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
            kind: MemberKind::Class(class),
            ..
        }) => {
            if checker.enabled(Rule::UndocumentedPublicClass) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(UndocumentedPublicClass, class.identifier()));
            }
            false
        }
        Definition::Member(Member {
            kind: MemberKind::NestedClass(function),
            ..
        }) => {
            if checker.enabled(Rule::UndocumentedPublicNestedClass) {
                checker.diagnostics.push(Diagnostic::new(
                    UndocumentedPublicNestedClass,
                    function.identifier(),
                ));
            }
            false
        }
        Definition::Member(Member {
            kind: MemberKind::Function(function) | MemberKind::NestedFunction(function),
            ..
        }) => {
            if is_overload(&function.decorator_list, checker.semantic()) {
                true
            } else {
                if checker.enabled(Rule::UndocumentedPublicFunction) {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicFunction,
                        function.identifier(),
                    ));
                }
                false
            }
        }
        Definition::Member(Member {
            kind: MemberKind::Method(function),
            ..
        }) => {
            if is_overload(&function.decorator_list, checker.semantic())
                || is_override(&function.decorator_list, checker.semantic())
            {
                true
            } else if is_init(&function.name) {
                if checker.enabled(Rule::UndocumentedPublicInit) {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicInit,
                        function.identifier(),
                    ));
                }
                true
            } else if is_new(&function.name) || is_call(&function.name) {
                if checker.enabled(Rule::UndocumentedPublicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicMethod,
                        function.identifier(),
                    ));
                }
                true
            } else if is_magic(&function.name) {
                if checker.enabled(Rule::UndocumentedMagicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedMagicMethod,
                        function.identifier(),
                    ));
                }
                true
            } else {
                if checker.enabled(Rule::UndocumentedPublicMethod) {
                    checker.diagnostics.push(Diagnostic::new(
                        UndocumentedPublicMethod,
                        function.identifier(),
                    ));
                }
                true
            }
        }
    }
}
