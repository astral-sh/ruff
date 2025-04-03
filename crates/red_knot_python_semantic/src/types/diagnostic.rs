use super::context::InferContext;
use crate::declare_lint;
use crate::lint::{Level, LintRegistryBuilder, LintStatus};
use crate::suppression::FileSuppressionId;
use crate::types::string_annotation::{
    BYTE_STRING_TYPE_ANNOTATION, ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION, FSTRING_TYPE_ANNOTATION,
    IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION, INVALID_SYNTAX_IN_FORWARD_ANNOTATION,
    RAW_STRING_TYPE_ANNOTATION,
};
use crate::types::{ClassLiteralType, KnownInstanceType, Type};
use ruff_db::diagnostic::{Diagnostic, OldSecondaryDiagnosticMessage, Span};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::Ranged;
use rustc_hash::FxHashSet;
use std::fmt::Formatter;

/// Registers all known type check lints.
pub(crate) fn register_lints(registry: &mut LintRegistryBuilder) {
    registry.register_lint(&CALL_NON_CALLABLE);
    registry.register_lint(&CALL_POSSIBLY_UNBOUND_METHOD);
    registry.register_lint(&CONFLICTING_ARGUMENT_FORMS);
    registry.register_lint(&CONFLICTING_DECLARATIONS);
    registry.register_lint(&CONFLICTING_METACLASS);
    registry.register_lint(&CYCLIC_CLASS_DEFINITION);
    registry.register_lint(&DIVISION_BY_ZERO);
    registry.register_lint(&DUPLICATE_BASE);
    registry.register_lint(&INCOMPATIBLE_SLOTS);
    registry.register_lint(&INCONSISTENT_MRO);
    registry.register_lint(&INDEX_OUT_OF_BOUNDS);
    registry.register_lint(&INVALID_ARGUMENT_TYPE);
    registry.register_lint(&INVALID_RETURN_TYPE);
    registry.register_lint(&INVALID_ASSIGNMENT);
    registry.register_lint(&INVALID_BASE);
    registry.register_lint(&INVALID_CONTEXT_MANAGER);
    registry.register_lint(&INVALID_DECLARATION);
    registry.register_lint(&INVALID_EXCEPTION_CAUGHT);
    registry.register_lint(&INVALID_METACLASS);
    registry.register_lint(&INVALID_PARAMETER_DEFAULT);
    registry.register_lint(&INVALID_RAISE);
    registry.register_lint(&INVALID_TYPE_CHECKING_CONSTANT);
    registry.register_lint(&INVALID_TYPE_FORM);
    registry.register_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS);
    registry.register_lint(&MISSING_ARGUMENT);
    registry.register_lint(&NO_MATCHING_OVERLOAD);
    registry.register_lint(&NON_SUBSCRIPTABLE);
    registry.register_lint(&NOT_ITERABLE);
    registry.register_lint(&UNSUPPORTED_BOOL_CONVERSION);
    registry.register_lint(&PARAMETER_ALREADY_ASSIGNED);
    registry.register_lint(&POSSIBLY_UNBOUND_ATTRIBUTE);
    registry.register_lint(&POSSIBLY_UNBOUND_IMPORT);
    registry.register_lint(&POSSIBLY_UNRESOLVED_REFERENCE);
    registry.register_lint(&SUBCLASS_OF_FINAL_CLASS);
    registry.register_lint(&TYPE_ASSERTION_FAILURE);
    registry.register_lint(&TOO_MANY_POSITIONAL_ARGUMENTS);
    registry.register_lint(&UNDEFINED_REVEAL);
    registry.register_lint(&UNKNOWN_ARGUMENT);
    registry.register_lint(&UNRESOLVED_ATTRIBUTE);
    registry.register_lint(&UNRESOLVED_IMPORT);
    registry.register_lint(&UNRESOLVED_REFERENCE);
    registry.register_lint(&UNSUPPORTED_OPERATOR);
    registry.register_lint(&ZERO_STEPSIZE_IN_SLICE);
    registry.register_lint(&STATIC_ASSERT_ERROR);
    registry.register_lint(&INVALID_ATTRIBUTE_ACCESS);
    registry.register_lint(&REDUNDANT_CAST);

    // String annotations
    registry.register_lint(&BYTE_STRING_TYPE_ANNOTATION);
    registry.register_lint(&ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION);
    registry.register_lint(&FSTRING_TYPE_ANNOTATION);
    registry.register_lint(&IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION);
    registry.register_lint(&INVALID_SYNTAX_IN_FORWARD_ANNOTATION);
    registry.register_lint(&RAW_STRING_TYPE_ANNOTATION);
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to non-callable objects.
    ///
    /// ## Why is this bad?
    /// Calling a non-callable object will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// 4()  # TypeError: 'int' object is not callable
    /// ```
    pub(crate) static CALL_NON_CALLABLE = {
        summary: "detects calls to non-callable objects",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to possibly unbound methods.
    ///
    /// TODO #14889
    pub(crate) static CALL_POSSIBLY_UNBOUND_METHOD = {
        summary: "detects calls to possibly unbound methods",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks whether an argument is used as both a value and a type form in a call
    pub(crate) static CONFLICTING_ARGUMENT_FORMS = {
        summary: "detects when an argument is used as both a value and a type form in a call",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static CONFLICTING_DECLARATIONS = {
        summary: "detects conflicting declarations",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static CONFLICTING_METACLASS = {
        summary: "detects conflicting metaclasses",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for class definitions with a cyclic inheritance chain.
    ///
    /// ## Why is it bad?
    /// TODO #14889
    pub(crate) static CYCLIC_CLASS_DEFINITION = {
        summary: "detects cyclic class definitions",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// It detects division by zero.
    ///
    /// ## Why is this bad?
    /// Dividing by zero raises a `ZeroDivisionError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// 5 / 0
    /// ```
    pub(crate) static DIVISION_BY_ZERO = {
        summary: "detects division by zero",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static DUPLICATE_BASE = {
        summary: "detects class definitions with duplicate bases",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for classes whose bases define incompatible `__slots__`.
    ///
    /// ## Why is this bad?
    /// Inheriting from bases with incompatible `__slots__`s
    /// will lead to a `TypeError` at runtime.
    ///
    /// Classes with no or empty `__slots__` are always compatible:
    ///
    /// ```python
    /// class A: ...
    /// class B:
    ///     __slots__ = ()
    /// class C:
    ///     __slots__ = ("a", "b")
    ///
    /// # fine
    /// class D(A, B, C): ...
    /// ```
    ///
    /// Multiple inheritance from more than one different class
    /// defining non-empty `__slots__` is not allowed:
    ///
    /// ```python
    /// class A:
    ///     __slots__ = ("a", "b")
    ///
    /// class B:
    ///     __slots__ = ("a", "b")  # Even if the values are the same
    ///
    /// # TypeError: multiple bases have instance lay-out conflict
    /// class C(A, B): ...
    /// ```
    ///
    /// ## Known problems
    /// Dynamic (not tuple or string literal) `__slots__` are not checked.
    /// Additionally, classes inheriting from built-in classes with implicit layouts
    /// like `str` or `int` are also not checked.
    ///
    /// ```pycon
    /// >>> hasattr(int, "__slots__")
    /// False
    /// >>> hasattr(str, "__slots__")
    /// False
    /// >>> class A(int, str): ...
    /// Traceback (most recent call last):
    ///   File "<python-input-0>", line 1, in <module>
    ///     class A(int, str): ...
    /// TypeError: multiple bases have instance lay-out conflict
    /// ```
    pub(crate) static INCOMPATIBLE_SLOTS = {
        summary: "detects class definitions whose MRO has conflicting `__slots__`",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INCONSISTENT_MRO = {
        summary: "detects class definitions with an inconsistent MRO",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// TODO #14889
    pub(crate) static INDEX_OUT_OF_BOUNDS = {
        summary: "detects index out of bounds errors",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Detects call arguments whose type is not assignable to the corresponding typed parameter.
    ///
    /// ## Why is this bad?
    /// Passing an argument of a type the function (or callable object) does not accept violates
    /// the expectations of the function author and may cause unexpected runtime errors within the
    /// body of the function.
    ///
    /// ## Examples
    /// ```python
    /// def func(x: int): ...
    /// func("foo")  # error: [invalid-argument-type]
    /// ```
    pub(crate) static INVALID_ARGUMENT_TYPE = {
        summary: "detects call arguments whose type is not assignable to the corresponding typed parameter",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Detects returned values that can't be assigned to the function's annotated return type.
    ///
    /// ## Why is this bad?
    /// Returning an object of a type incompatible with the annotated return type may cause confusion to the user calling the function.
    ///
    /// ## Examples
    /// ```python
    /// def func() -> int:
    ///     return "a"  # error: [invalid-return-type]
    /// ```
    pub(crate) static INVALID_RETURN_TYPE = {
        summary: "detects returned values that can't be assigned to the function's annotated return type",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_ASSIGNMENT = {
        summary: "detects invalid assignments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_BASE = {
        summary: "detects class definitions with an invalid base",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_CONTEXT_MANAGER = {
        summary: "detects expressions used in with statements that don't implement the context manager protocol",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_DECLARATION = {
        summary: "detects invalid declarations",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for exception handlers that catch non-exception classes.
    ///
    /// ## Why is this bad?
    /// Catching classes that do not inherit from `BaseException` will raise a TypeError at runtime.
    ///
    /// ## Example
    /// ```python
    /// try:
    ///     1 / 0
    /// except 1:
    ///     ...
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// try:
    ///     1 / 0
    /// except ZeroDivisionError:
    ///     ...
    /// ```
    ///
    /// ## References
    /// - [Python documentation: except clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
    /// - [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)
    ///
    /// ## Ruff rule
    ///  This rule corresponds to Ruff's [`except-with-non-exception-classes` (`B030`)](https://docs.astral.sh/ruff/rules/except-with-non-exception-classes)
    pub(crate) static INVALID_EXCEPTION_CAUGHT = {
        summary: "detects exception handlers that catch classes that do not inherit from `BaseException`",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for arguments to `metaclass=` that are invalid.
    ///
    /// ## Why is this bad?
    /// Python allows arbitrary expressions to be used as the argument to `metaclass=`.
    /// These expressions, however, need to be callable and accept the same arguments
    /// as `type.__new__`.
    ///
    /// ## Example
    ///
    /// ```python
    /// def f(): ...
    ///
    /// # TypeError: f() takes 0 positional arguments but 3 were given
    /// class B(metaclass=f): ...
    /// ```
    ///
    /// ## References
    /// - [Python documentation: Metaclasses](https://docs.python.org/3/reference/datamodel.html#metaclasses)
    pub(crate) static INVALID_METACLASS = {
        summary: "detects invalid `metaclass=` arguments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for default values that can't be assigned to the parameter's annotated type.
    ///
    /// ## Why is this bad?
    /// TODO #14889
    pub(crate) static INVALID_PARAMETER_DEFAULT = {
        summary: "detects default values that can't be assigned to the parameter's annotated type",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// Checks for `raise` statements that raise non-exceptions or use invalid
    /// causes for their raised exceptions.
    ///
    /// ## Why is this bad?
    /// Only subclasses or instances of `BaseException` can be raised.
    /// For an exception's cause, the same rules apply, except that `None` is also
    /// permitted. Violating these rules results in a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// def f():
    ///     try:
    ///         something()
    ///     except NameError:
    ///         raise "oops!" from f
    ///
    /// def g():
    ///     raise NotImplemented from 42
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def f():
    ///     try:
    ///         something()
    ///     except NameError as e:
    ///         raise RuntimeError("oops!") from e
    ///
    /// def g():
    ///     raise NotImplementedError from None
    /// ```
    ///
    /// ## References
    /// - [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#raise)
    /// - [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)
    pub(crate) static INVALID_RAISE = {
        summary: "detects `raise` statements that raise invalid exceptions or use invalid causes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for a value other than `False` assigned to the `TYPE_CHECKING` variable, or an
    /// annotation not assignable from `bool`.
    ///
    /// ## Why is this bad?
    /// The name `TYPE_CHECKING` is reserved for a flag that can be used to provide conditional
    /// code seen only by the type checker, and not at runtime. Normally this flag is imported from
    /// `typing` or `typing_extensions`, but it can also be defined locally. If defined locally, it
    /// must be assigned the value `False` at runtime; the type checker will consider its value to
    /// be `True`. If annotated, it must be annotated as a type that can accept `bool` values.
    pub(crate) static INVALID_TYPE_CHECKING_CONSTANT = {
        summary: "detects invalid TYPE_CHECKING constant assignments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for invalid type expressions.
    ///
    /// ## Why is this bad?
    /// TODO #14889
    pub(crate) static INVALID_TYPE_FORM = {
        summary: "detects invalid type forms",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_TYPE_VARIABLE_CONSTRAINTS = {
        summary: "detects invalid type variable constraints",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for missing required arguments in a call.
    ///
    /// ## Why is this bad?
    /// Failing to provide a required argument will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// def func(x: int): ...
    /// func()  # TypeError: func() missing 1 required positional argument: 'x'
    /// ```
    pub(crate) static MISSING_ARGUMENT = {
        summary: "detects missing required arguments in a call",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to an overloaded function that do not match any of the overloads.
    ///
    /// ## Why is this bad?
    /// Failing to provide the correct arguments to one of the overloads will raise a `TypeError`
    /// at runtime.
    ///
    /// ## Examples
    /// ```python
    /// @overload
    /// def func(x: int): ...
    /// @overload
    /// def func(x: bool): ...
    /// func("string")  # error: [no-matching-overload]
    /// ```
    pub(crate) static NO_MATCHING_OVERLOAD = {
        summary: "detects calls that do not match any overload",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for subscripting objects that do not support subscripting.
    ///
    /// ## Why is this bad?
    /// Subscripting an object that does not support it will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// 4[1]  # TypeError: 'int' object is not subscriptable
    /// ```
    pub(crate) static NON_SUBSCRIPTABLE = {
        summary: "detects subscripting objects that do not support subscripting",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for objects that are not iterable but are used in a context that requires them to be.
    ///
    /// ## Why is this bad?
    /// Iterating over an object that is not iterable will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    ///
    /// ```python
    /// for i in 34:  # TypeError: 'int' object is not iterable
    ///     pass
    /// ```
    pub(crate) static NOT_ITERABLE = {
        summary: "detects iteration over an object that is not iterable",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for bool conversions where the object doesn't correctly implement `__bool__`.
    ///
    /// ## Why is this bad?
    /// If an exception is raised when you attempt to evaluate the truthiness of an object,
    /// using the object in a boolean context will fail at runtime.
    ///
    /// ## Examples
    ///
    /// ```python
    /// class NotBoolable:
    ///     __bool__ = None
    ///
    /// b1 = NotBoolable()
    /// b2 = NotBoolable()
    ///
    /// if b1:  # exception raised here
    ///     pass
    ///
    /// b1 and b2  # exception raised here
    /// not b1  # exception raised here
    /// b1 < b2 < b1  # exception raised here
    /// ```
    pub(crate) static UNSUPPORTED_BOOL_CONVERSION = {
        summary: "detects boolean conversion where the object incorrectly implements `__bool__`",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls which provide more than one argument for a single parameter.
    ///
    /// ## Why is this bad?
    /// Providing multiple values for a single parameter will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    ///
    /// ```python
    /// def f(x: int) -> int: ...
    ///
    /// f(1, x=2)  # Error raised here
    /// ```
    pub(crate) static PARAMETER_ALREADY_ASSIGNED = {
        summary: "detects multiple arguments for the same parameter",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for possibly unbound attributes.
    ///
    /// TODO #14889
    pub(crate) static POSSIBLY_UNBOUND_ATTRIBUTE = {
        summary: "detects references to possibly unbound attributes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static POSSIBLY_UNBOUND_IMPORT = {
        summary: "detects possibly unbound imports",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for references to names that are possibly not defined.
    ///
    /// ## Why is this bad?
    /// Using an undefined variable will raise a `NameError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// for i in range(0):
    ///     x = i
    ///
    /// print(x)  # NameError: name 'x' is not defined
    /// ```
    pub(crate) static POSSIBLY_UNRESOLVED_REFERENCE = {
        summary: "detects references to possibly undefined names",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for classes that subclass final classes.
    ///
    /// ## Why is this bad?
    /// Decorating a class with `@final` declares to the type checker that it should not be subclassed.
    ///
    /// ## Example
    ///
    /// ```python
    /// from typing import final
    ///
    /// @final
    /// class A: ...
    /// class B(A): ...  # Error raised here
    /// ```
    pub(crate) static SUBCLASS_OF_FINAL_CLASS = {
        summary: "detects subclasses of final classes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for `assert_type()` calls where the actual type
    /// is not the same as the asserted type.
    ///
    /// ## Why is this bad?
    /// `assert_type()` allows confirming the inferred type of a certain value.
    ///
    /// ## Example
    ///
    /// ```python
    /// def _(x: int):
    ///     assert_type(x, int)  # fine
    ///     assert_type(x, str)  # error: Actual type does not match asserted type
    /// ```
    pub(crate) static TYPE_ASSERTION_FAILURE = {
        summary: "detects failed type assertions",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls that pass more positional arguments than the callable can accept.
    ///
    /// ## Why is this bad?
    /// Passing too many positional arguments will raise `TypeError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// def f(): ...
    ///
    /// f("foo")  # Error raised here
    /// ```
    pub(crate) static TOO_MANY_POSITIONAL_ARGUMENTS = {
        summary: "detects calls passing too many positional arguments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to `reveal_type` without importing it.
    ///
    /// ## Why is this bad?
    /// Using `reveal_type` without importing it will raise a `NameError` at runtime.
    ///
    /// ## Examples
    /// TODO #14889
    pub(crate) static UNDEFINED_REVEAL = {
        summary: "detects usages of `reveal_type` without importing it",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for keyword arguments in calls that don't match any parameter of the callable.
    ///
    /// ## Why is this bad?
    /// Providing an unknown keyword argument will raise `TypeError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// def f(x: int) -> int: ...
    ///
    /// f(x=1, y=2)  # Error raised here
    /// ```
    pub(crate) static UNKNOWN_ARGUMENT = {
        summary: "detects unknown keyword arguments in calls",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for unresolved attributes.
    ///
    /// TODO #14889
    pub(crate) static UNRESOLVED_ATTRIBUTE = {
        summary: "detects references to unresolved attributes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for import statements for which the module cannot be resolved.
    ///
    /// ## Why is this bad?
    /// Importing a module that cannot be resolved will raise an `ImportError` at runtime.
    pub(crate) static UNRESOLVED_IMPORT = {
        summary: "detects unresolved imports",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for references to names that are not defined.
    ///
    /// ## Why is this bad?
    /// Using an undefined variable will raise a `NameError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// print(x)  # NameError: name 'x' is not defined
    /// ```
    pub(crate) static UNRESOLVED_REFERENCE = {
        summary: "detects references to names that are not defined",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for binary expressions, comparisons, and unary expressions where the operands don't support the operator.
    ///
    /// TODO #14889
    pub(crate) static UNSUPPORTED_OPERATOR = {
        summary: "detects binary, unary, or comparison expressions where the operands don't support the operator",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for step size 0 in slices.
    ///
    /// ## Why is this bad?
    /// A slice with a step size of zero will raise a `ValueError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// l = list(range(10))
    /// l[1:10:0]  # ValueError: slice step cannot be zero
    /// ```
    pub(crate) static ZERO_STEPSIZE_IN_SLICE = {
        summary: "detects a slice step size of zero",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Makes sure that the argument of `static_assert` is statically known to be true.
    ///
    /// ## Examples
    /// ```python
    /// from knot_extensions import static_assert
    ///
    /// static_assert(1 + 1 == 3)  # error: evaluates to `False`
    ///
    /// static_assert(int(2.0 * 3.0) == 6)  # error: does not have a statically known truthiness
    /// ```
    pub(crate) static STATIC_ASSERT_ERROR = {
        summary: "Failed static assertion",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Makes sure that instance attribute accesses are valid.
    ///
    /// ## Examples
    /// ```python
    /// class C:
    ///   var: ClassVar[int] = 1
    ///
    /// C.var = 3  # okay
    /// C().var = 3  # error: Cannot assign to class variable
    /// ```
    pub(crate) static INVALID_ATTRIBUTE_ACCESS = {
        summary: "Invalid attribute access",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Detects redundant `cast` calls where the value already has the target type.
    ///
    /// ## Why is this bad?
    /// These casts have no effect and can be removed.
    ///
    /// ## Example
    /// ```python
    /// def f() -> int:
    ///     return 10
    ///
    /// cast(int, f())  # Redundant
    /// ```
    pub(crate) static REDUNDANT_CAST = {
        summary: "detects redundant `cast` calls",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

/// A collection of type check diagnostics.
#[derive(Default, Eq, PartialEq)]
pub struct TypeCheckDiagnostics {
    diagnostics: Vec<Diagnostic>,
    used_suppressions: FxHashSet<FileSuppressionId>,
}

impl TypeCheckDiagnostics {
    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn extend(&mut self, other: &TypeCheckDiagnostics) {
        self.diagnostics.extend_from_slice(&other.diagnostics);
        self.used_suppressions.extend(&other.used_suppressions);
    }

    pub(crate) fn mark_used(&mut self, suppression_id: FileSuppressionId) {
        self.used_suppressions.insert(suppression_id);
    }

    pub(crate) fn is_used(&self, suppression_id: FileSuppressionId) -> bool {
        self.used_suppressions.contains(&suppression_id)
    }

    pub(crate) fn used_len(&self) -> usize {
        self.used_suppressions.len()
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.used_suppressions.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.diagnostics.iter()
    }
}

impl std::fmt::Debug for TypeCheckDiagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.diagnostics.fmt(f)
    }
}

impl IntoIterator for TypeCheckDiagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.into_iter()
    }
}

impl<'a> IntoIterator for &'a TypeCheckDiagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.iter()
    }
}

/// Emit a diagnostic declaring that an index is out of bounds for a tuple.
pub(super) fn report_index_out_of_bounds(
    context: &InferContext,
    kind: &'static str,
    node: AnyNodeRef,
    tuple_ty: Type,
    length: usize,
    index: i64,
) {
    context.report_lint(
        &INDEX_OUT_OF_BOUNDS,
        node,
        format_args!(
            "Index {index} is out of bounds for {kind} `{}` with length {length}",
            tuple_ty.display(context.db())
        ),
    );
}

/// Emit a diagnostic declaring that a type does not support subscripting.
pub(super) fn report_non_subscriptable(
    context: &InferContext,
    node: AnyNodeRef,
    non_subscriptable_ty: Type,
    method: &str,
) {
    context.report_lint(
        &NON_SUBSCRIPTABLE,
        node,
        format_args!(
            "Cannot subscript object of type `{}` with no `{method}` method",
            non_subscriptable_ty.display(context.db())
        ),
    );
}

pub(super) fn report_unresolved_module<'db>(
    context: &InferContext,
    import_node: impl Into<AnyNodeRef<'db>>,
    level: u32,
    module: Option<&str>,
) {
    context.report_lint(
        &UNRESOLVED_IMPORT,
        import_node.into(),
        format_args!(
            "Cannot resolve import `{}{}`",
            ".".repeat(level as usize),
            module.unwrap_or_default()
        ),
    );
}

pub(super) fn report_slice_step_size_zero(context: &InferContext, node: AnyNodeRef) {
    context.report_lint(
        &ZERO_STEPSIZE_IN_SLICE,
        node,
        format_args!("Slice step size can not be zero"),
    );
}

fn report_invalid_assignment_with_message(
    context: &InferContext,
    node: AnyNodeRef,
    target_ty: Type,
    message: std::fmt::Arguments,
) {
    match target_ty {
        Type::ClassLiteral(ClassLiteralType { class }) => {
            context.report_lint(&INVALID_ASSIGNMENT, node, format_args!(
                    "Implicit shadowing of class `{}`; annotate to make it explicit if this is intentional",
                    class.name(context.db())));
        }
        Type::FunctionLiteral(function) => {
            context.report_lint(&INVALID_ASSIGNMENT, node, format_args!(
                    "Implicit shadowing of function `{}`; annotate to make it explicit if this is intentional",
                    function.name(context.db())));
        }
        _ => {
            context.report_lint(&INVALID_ASSIGNMENT, node, message);
        }
    }
}

pub(super) fn report_invalid_assignment(
    context: &InferContext,
    node: AnyNodeRef,
    target_ty: Type,
    source_ty: Type,
) {
    report_invalid_assignment_with_message(
        context,
        node,
        target_ty,
        format_args!(
            "Object of type `{}` is not assignable to `{}`",
            source_ty.display(context.db()),
            target_ty.display(context.db()),
        ),
    );
}

pub(super) fn report_invalid_attribute_assignment(
    context: &InferContext,
    node: AnyNodeRef,
    target_ty: Type,
    source_ty: Type,
    attribute_name: &'_ str,
) {
    report_invalid_assignment_with_message(
        context,
        node,
        target_ty,
        format_args!(
            "Object of type `{}` is not assignable to attribute `{attribute_name}` of type `{}`",
            source_ty.display(context.db()),
            target_ty.display(context.db()),
        ),
    );
}

pub(super) fn report_invalid_return_type(
    context: &InferContext,
    object_range: impl Ranged,
    return_type_range: impl Ranged,
    expected_ty: Type,
    actual_ty: Type,
) {
    let return_type_span = Span::from(context.file()).with_range(return_type_range.range());
    context.report_lint_with_secondary_messages(
        &INVALID_RETURN_TYPE,
        object_range,
        format_args!(
            "Object of type `{}` is not assignable to return type `{}`",
            actual_ty.display(context.db()),
            expected_ty.display(context.db())
        ),
        &[OldSecondaryDiagnosticMessage::new(
            return_type_span,
            format!(
                "Return type is declared here as `{}`",
                expected_ty.display(context.db())
            ),
        )],
    );
}

pub(super) fn report_implicit_return_type(
    context: &InferContext,
    range: impl Ranged,
    expected_ty: Type,
) {
    context.report_lint(
        &INVALID_RETURN_TYPE,
        range,
        format_args!(
            "Function can implicitly return `None`, which is not assignable to return type `{}`",
            expected_ty.display(context.db())
        ),
    );
}

pub(super) fn report_invalid_type_checking_constant(context: &InferContext, node: AnyNodeRef) {
    context.report_lint(
        &INVALID_TYPE_CHECKING_CONSTANT,
        node,
        format_args!("The name TYPE_CHECKING is reserved for use as a flag; only False can be assigned to it.",),
    );
}

pub(super) fn report_possibly_unresolved_reference(
    context: &InferContext,
    expr_name_node: &ast::ExprName,
) {
    let ast::ExprName { id, .. } = expr_name_node;

    context.report_lint(
        &POSSIBLY_UNRESOLVED_REFERENCE,
        expr_name_node,
        format_args!("Name `{id}` used when possibly not defined"),
    );
}

pub(super) fn report_possibly_unbound_attribute(
    context: &InferContext,
    target: &ast::ExprAttribute,
    attribute: &str,
    object_ty: Type,
) {
    context.report_lint(
        &POSSIBLY_UNBOUND_ATTRIBUTE,
        target,
        format_args!(
            "Attribute `{attribute}` on type `{}` is possibly unbound",
            object_ty.display(context.db()),
        ),
    );
}

pub(super) fn report_unresolved_reference(context: &InferContext, expr_name_node: &ast::ExprName) {
    let ast::ExprName { id, .. } = expr_name_node;

    context.report_lint(
        &UNRESOLVED_REFERENCE,
        expr_name_node,
        format_args!("Name `{id}` used when not defined"),
    );
}

pub(super) fn report_invalid_exception_caught(context: &InferContext, node: &ast::Expr, ty: Type) {
    context.report_lint(
        &INVALID_EXCEPTION_CAUGHT,
        node,
        format_args!(
            "Cannot catch object of type `{}` in an exception handler \
            (must be a `BaseException` subclass or a tuple of `BaseException` subclasses)",
            ty.display(context.db())
        ),
    );
}

pub(crate) fn report_invalid_exception_raised(context: &InferContext, node: &ast::Expr, ty: Type) {
    context.report_lint(
        &INVALID_RAISE,
        node,
        format_args!(
            "Cannot raise object of type `{}` (must be a `BaseException` subclass or instance)",
            ty.display(context.db())
        ),
    );
}

pub(crate) fn report_invalid_exception_cause(context: &InferContext, node: &ast::Expr, ty: Type) {
    context.report_lint(
        &INVALID_RAISE,
        node,
        format_args!(
            "Cannot use object of type `{}` as exception cause \
            (must be a `BaseException` subclass or instance or `None`)",
            ty.display(context.db())
        ),
    );
}

pub(crate) fn report_base_with_incompatible_slots(context: &InferContext, node: &ast::Expr) {
    context.report_lint(
        &INCOMPATIBLE_SLOTS,
        node,
        format_args!("Class base has incompatible `__slots__`"),
    );
}

pub(crate) fn report_invalid_arguments_to_annotated(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    context.report_lint(
        &INVALID_TYPE_FORM,
        subscript,
        format_args!(
            "Special form `{}` expected at least 2 arguments (one type and at least one metadata element)",
            KnownInstanceType::Annotated.repr(context.db())
        ),
    );
}

pub(crate) fn report_invalid_arguments_to_callable(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    context.report_lint(
        &INVALID_TYPE_FORM,
        subscript,
        format_args!(
            "Special form `{}` expected exactly two arguments (parameter types and return type)",
            KnownInstanceType::Callable.repr(context.db())
        ),
    );
}
