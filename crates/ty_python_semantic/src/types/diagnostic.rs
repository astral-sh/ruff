use super::call::CallErrorKind;
use super::context::InferContext;
use super::mro::DuplicateBaseError;
use super::{CallArgumentTypes, CallDunderError, ClassBase, ClassLiteral, KnownClass};
use crate::db::Db;
use crate::lint::{Level, LintRegistryBuilder, LintStatus};
use crate::suppression::FileSuppressionId;
use crate::types::LintDiagnosticGuard;
use crate::types::string_annotation::{
    BYTE_STRING_TYPE_ANNOTATION, ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION, FSTRING_TYPE_ANNOTATION,
    IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION, INVALID_SYNTAX_IN_FORWARD_ANNOTATION,
    RAW_STRING_TYPE_ANNOTATION,
};
use crate::types::{KnownFunction, KnownInstanceType, Type, protocol_class::ProtocolClassLiteral};
use crate::{Program, PythonVersionWithSource, declare_lint};
use itertools::Itertools;
use ruff_db::diagnostic::{Annotation, Diagnostic, Severity, Span, SubDiagnostic};
use ruff_db::files::system_path_to_file;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_stdlib::builtins::version_builtin_was_added;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use std::fmt::Formatter;

/// Registers all known type check lints.
pub(crate) fn register_lints(registry: &mut LintRegistryBuilder) {
    registry.register_lint(&CALL_NON_CALLABLE);
    registry.register_lint(&POSSIBLY_UNBOUND_IMPLICIT_CALL);
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
    registry.register_lint(&INVALID_GENERIC_CLASS);
    registry.register_lint(&INVALID_LEGACY_TYPE_VARIABLE);
    registry.register_lint(&INVALID_TYPE_ALIAS_TYPE);
    registry.register_lint(&INVALID_METACLASS);
    registry.register_lint(&INVALID_OVERLOAD);
    registry.register_lint(&INVALID_PARAMETER_DEFAULT);
    registry.register_lint(&INVALID_PROTOCOL);
    registry.register_lint(&INVALID_RAISE);
    registry.register_lint(&INVALID_SUPER_ARGUMENT);
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
    registry.register_lint(&UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS);
    registry.register_lint(&UNDEFINED_REVEAL);
    registry.register_lint(&UNKNOWN_ARGUMENT);
    registry.register_lint(&UNRESOLVED_ATTRIBUTE);
    registry.register_lint(&UNRESOLVED_IMPORT);
    registry.register_lint(&UNRESOLVED_REFERENCE);
    registry.register_lint(&UNSUPPORTED_BASE);
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
    /// Checks for implicit calls to possibly unbound methods.
    ///
    /// ## Why is this bad?
    /// Expressions such as `x[y]` and `x * y` call methods
    /// under the hood (`__getitem__` and `__mul__` respectively).
    /// Calling an unbound method will raise an `AttributeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// import datetime
    ///
    /// class A:
    ///     if datetime.date.today().weekday() != 6:
    ///         def __getitem__(self, v): ...
    ///
    /// A()[0]  # TypeError: 'A' object is not subscriptable
    /// ```
    pub(crate) static POSSIBLY_UNBOUND_IMPLICIT_CALL = {
        summary: "detects implicit calls to possibly unbound methods",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks whether an argument is used as both a value and a type form in a call.
    ///
    /// ## Why is this bad?
    /// Such calls have confusing semantics and often indicate a logic error.
    ///
    /// ## Examples
    /// ```python
    /// from typing import reveal_type
    /// from ty_extensions import is_fully_static
    ///
    /// if flag:
    ///     f = repr  # Expects a value
    /// else:
    ///     f = is_fully_static  # Expects a type form
    ///
    /// f(int)  # error
    /// ```
    pub(crate) static CONFLICTING_ARGUMENT_FORMS = {
        summary: "detects when an argument is used as both a value and a type form in a call",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks whether a variable has been declared as two conflicting types.
    ///
    /// ## Why is this bad
    /// A variable with two conflicting declarations likely indicates a mistake.
    /// Moreover, it could lead to incorrect or ill-defined type inference for
    /// other code that relies on these variables.
    ///
    /// ## Examples
    /// ```python
    /// if b:
    ///     a: int
    /// else:
    ///     a: str
    ///
    /// a = 1
    /// ```
    pub(crate) static CONFLICTING_DECLARATIONS = {
        summary: "detects conflicting declarations",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for class definitions where the metaclass of the class
    /// being created would not be a subclass of the metaclasses of
    /// all the class's bases.
    ///
    /// ## Why is it bad?
    /// Such a class definition raises a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// class M1(type): ...
    /// class M2(type): ...
    /// class A(metaclass=M1): ...
    /// class B(metaclass=M2): ...
    ///
    /// # TypeError: metaclass conflict
    /// class C(A, B): ...
    /// ```
    pub(crate) static CONFLICTING_METACLASS = {
        summary: "detects conflicting metaclasses",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for class definitions in stub files that inherit
    /// (directly or indirectly) from themselves.
    ///
    /// ## Why is it bad?
    /// Although forward references are natively supported in stub files,
    /// inheritance cycles are still disallowed, as it is impossible to
    /// resolve a consistent [method resolution order] for a class that
    /// inherits from itself.
    ///
    /// ## Examples
    /// ```python
    /// # foo.pyi
    /// class A(B): ...
    /// class B(A): ...
    /// ```
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
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
        default_level: Level::Ignore,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for class definitions with duplicate bases.
    ///
    /// ## Why is this bad?
    /// Class definitions with duplicate bases raise `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// class A: ...
    ///
    /// # TypeError: duplicate base class
    /// class B(A, A): ...
    /// ```
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
    /// ## What it does
    /// Checks for invalidly defined protocol classes.
    ///
    /// ## Why is this bad?
    /// An invalidly defined protocol class may lead to the type checker inferring
    /// unexpected things. It may also lead to `TypeError`s at runtime.
    ///
    /// ## Examples
    /// A `Protocol` class cannot inherit from a non-`Protocol` class;
    /// this raises a `TypeError` at runtime:
    ///
    /// ```pycon
    /// >>> from typing import Protocol
    /// >>> class Foo(int, Protocol): ...
    /// ...
    /// Traceback (most recent call last):
    ///   File "<python-input-1>", line 1, in <module>
    ///     class Foo(int, Protocol): ...
    /// TypeError: Protocols can only inherit from other protocols, got <class 'int'>
    /// ```
    pub(crate) static INVALID_PROTOCOL = {
        summary: "detects invalid protocol class definitions",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for classes with an inconsistent [method resolution order] (MRO).
    ///
    /// ## Why is this bad?
    /// Classes with an inconsistent MRO will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// class A: ...
    /// class B(A): ...
    ///
    /// # TypeError: Cannot create a consistent method resolution order
    /// class C(A, B): ...
    /// ```
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(crate) static INCONSISTENT_MRO = {
        summary: "detects class definitions with an inconsistent MRO",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for attempts to use an out of bounds index to get an item from
    /// a container.
    ///
    /// ## Why is this bad?
    /// Using an out of bounds index will raise an `IndexError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// t = (0, 1, 2)
    /// t[3]  # IndexError: tuple index out of range
    /// ```
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
    /// ## What it does
    /// Checks for assignments where the type of the value
    /// is not [assignable to] the type of the assignee.
    ///
    /// ## Why is this bad?
    /// Such assignments break the rules of the type system and
    /// weaken a type checker's ability to accurately reason about your code.
    ///
    /// ## Examples
    /// ```python
    /// a: int = ''
    /// ```
    ///
    /// [assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
    pub(crate) static INVALID_ASSIGNMENT = {
        summary: "detects invalid assignments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for class definitions that have bases which are not instances of `type`.
    ///
    /// ## Why is this bad?
    /// Class definitions with bases like this will lead to `TypeError` being raised at runtime.
    ///
    /// ## Examples
    /// ```python
    /// class A(42): ...  # error: [invalid-base]
    /// ```
    pub(crate) static INVALID_BASE = {
        summary: "detects class bases that will cause the class definition to raise an exception at runtime",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for class definitions that have bases which are unsupported by ty.
    ///
    /// ## Why is this bad?
    /// If a class has a base that is an instance of a complex type such as a union type,
    /// ty will not be able to resolve the [method resolution order] (MRO) for the class.
    /// This will lead to an inferior understanding of your codebase and unpredictable
    /// type-checking behavior.
    ///
    /// ## Examples
    /// ```python
    /// import datetime
    ///
    /// class A: ...
    /// class B: ...
    ///
    /// if datetime.date.today().weekday() != 6:
    ///     C = A
    /// else:
    ///     C = B
    ///
    /// class D(C): ...  # error: [unsupported-base]
    /// ```
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(crate) static UNSUPPORTED_BASE = {
        summary: "detects class bases that are unsupported as ty could not feasibly calculate the class's MRO",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for expressions used in `with` statements
    /// that do not implement the context manager protocol.
    ///
    /// ## Why is this bad?
    /// Such a statement will raise `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// # TypeError: 'int' object does not support the context manager protocol
    /// with 1:
    ///     print(2)
    /// ```
    pub(crate) static INVALID_CONTEXT_MANAGER = {
        summary: "detects expressions used in with statements that don't implement the context manager protocol",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for declarations where the inferred type of an existing symbol
    /// is not [assignable to] its post-hoc declared type.
    ///
    /// ## Why is this bad?
    /// Such declarations break the rules of the type system and
    /// weaken a type checker's ability to accurately reason about your code.
    ///
    /// ## Examples
    /// ```python
    /// a = 1
    /// a: str
    /// ```
    ///
    /// [assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
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
    /// Checks for the creation of invalid generic classes
    ///
    /// ## Why is this bad?
    /// There are several requirements that you must follow when defining a generic class.
    ///
    /// ## Examples
    /// ```python
    /// from typing import Generic, TypeVar
    ///
    /// T = TypeVar("T")  # okay
    ///
    /// # error: class uses both PEP-695 syntax and legacy syntax
    /// class C[U](Generic[T]): ...
    /// ```
    ///
    /// ## References
    /// - [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)
    pub(crate) static INVALID_GENERIC_CLASS = {
        summary: "detects invalid generic classes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for the creation of invalid legacy `TypeVar`s
    ///
    /// ## Why is this bad?
    /// There are several requirements that you must follow when creating a legacy `TypeVar`.
    ///
    /// ## Examples
    /// ```python
    /// from typing import TypeVar
    ///
    /// T = TypeVar("T")  # okay
    /// Q = TypeVar("S")  # error: TypeVar name must match the variable it's assigned to
    /// T = TypeVar("T")  # error: TypeVars should not be redefined
    ///
    /// # error: TypeVar must be immediately assigned to a variable
    /// def f(t: TypeVar("U")): ...
    /// ```
    ///
    /// ## References
    /// - [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)
    pub(crate) static INVALID_LEGACY_TYPE_VARIABLE = {
        summary: "detects invalid legacy type variables",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for the creation of invalid `TypeAliasType`s
    ///
    /// ## Why is this bad?
    /// There are several requirements that you must follow when creating a `TypeAliasType`.
    ///
    /// ## Examples
    /// ```python
    /// from typing import TypeAliasType
    ///
    /// IntOrStr = TypeAliasType("IntOrStr", int | str)  # okay
    /// NewAlias = TypeAliasType(get_name(), int)        # error: TypeAliasType name must be a string literal
    /// ```
    pub(crate) static INVALID_TYPE_ALIAS_TYPE = {
        summary: "detects invalid TypeAliasType definitions",
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
    /// Checks for various invalid `@overload` usages.
    ///
    /// ## Why is this bad?
    /// The `@overload` decorator is used to define functions and methods that accepts different
    /// combinations of arguments and return different types based on the arguments passed. This is
    /// mainly beneficial for type checkers. But, if the `@overload` usage is invalid, the type
    /// checker may not be able to provide correct type information.
    ///
    /// ## Example
    ///
    /// Defining only one overload:
    ///
    /// ```py
    /// from typing import overload
    ///
    /// @overload
    /// def foo(x: int) -> int: ...
    /// def foo(x: int | None) -> int | None:
    ///     return x
    /// ```
    ///
    /// Or, not providing an implementation for the overloaded definition:
    ///
    /// ```py
    /// from typing import overload
    ///
    /// @overload
    /// def foo() -> None: ...
    /// @overload
    /// def foo(x: int) -> int: ...
    /// ```
    ///
    /// ## References
    /// - [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)
    pub(crate) static INVALID_OVERLOAD = {
        summary: "detects invalid `@overload` usages",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for default values that can't be
    /// assigned to the parameter's annotated type.
    ///
    /// ## Why is this bad?
    /// This breaks the rules of the type system and
    /// weakens a type checker's ability to accurately reason about your code.
    ///
    /// ## Examples
    /// ```python
    /// def f(a: int = ''): ...
    /// ```
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
    /// Detects `super()` calls where:
    /// - the first argument is not a valid class literal, or
    /// - the second argument is not an instance or subclass of the first argument.
    ///
    /// ## Why is this bad?
    /// `super(type, obj)` expects:
    /// - the first argument to be a class,
    /// - and the second argument to satisfy one of the following:
    ///   - `isinstance(obj, type)` is `True`
    ///   - `issubclass(obj, type)` is `True`
    ///
    /// Violating this relationship will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// class A:
    ///     ...
    /// class B(A):
    ///     ...
    ///
    /// super(A, B())  # it's okay! `A` satisfies `isinstance(B(), A)`
    ///
    /// super(A(), B()) # error: `A()` is not a class
    ///
    /// super(B, A())  # error: `A()` does not satisfy `isinstance(A(), B)`
    /// super(B, A)  # error: `A` does not satisfy `issubclass(A, B)`
    /// ```
    ///
    /// ## References
    /// - [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)
    pub(crate) static INVALID_SUPER_ARGUMENT = {
        summary: "detects invalid arguments for `super()`",
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
    ///
    /// ## Examples
    /// ```python
    /// TYPE_CHECKING: str
    /// TYPE_CHECKING = ''
    /// ```
    pub(crate) static INVALID_TYPE_CHECKING_CONSTANT = {
        summary: "detects invalid `TYPE_CHECKING` constant assignments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for expressions that are used as [type expressions]
    /// but cannot validly be interpreted as such.
    ///
    /// ## Why is this bad?
    /// Such expressions cannot be understood by ty.
    /// In some cases, they might raise errors at runtime.
    ///
    /// ## Examples
    /// ```python
    /// from typing import Annotated
    ///
    /// a: type[1]  # `1` is not a type
    /// b: Annotated[int]  # `Annotated` expects at least two arguments
    /// ```
    /// [type expressions]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
    pub(crate) static INVALID_TYPE_FORM = {
        summary: "detects invalid type forms",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for constrained [type variables] with only one constraint.
    ///
    /// ## Why is this bad?
    /// A constrained type variable must have at least two constraints.
    ///
    /// ## Examples
    /// ```python
    /// from typing import TypeVar
    ///
    /// T = TypeVar('T', str)  # invalid constrained TypeVar
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// T = TypeVar('T', str, int)  # valid constrained TypeVar
    /// # or
    /// T = TypeVar('T', bound=str)  # valid bound TypeVar
    /// ```
    ///
    /// [type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar
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
    /// ## Why is this bad?
    /// Attempting to access an unbound attribute will raise an `AttributeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// class A:
    ///     if b:
    ///         c = 0
    ///
    /// A.c  # AttributeError: type object 'A' has no attribute 'c'
    /// ```
    pub(crate) static POSSIBLY_UNBOUND_ATTRIBUTE = {
        summary: "detects references to possibly unbound attributes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for imports of symbols that may be unbound.
    ///
    /// ## Why is this bad?
    /// Importing an unbound module or name will raise a `ModuleNotFoundError`
    /// or `ImportError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// # module.py
    /// import datetime
    ///
    /// if datetime.date.today().weekday() != 6:
    ///     a = 1
    ///
    /// # main.py
    /// from module import a  # ImportError: cannot import name 'a' from 'module'
    /// ```
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
        default_level: Level::Ignore,
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
    /// Checks for `assert_type()` and `assert_never()` calls where the actual type
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
    /// Detects invalid `super()` calls where implicit arguments like the enclosing class or first method argument are unavailable.
    ///
    /// ## Why is this bad?
    /// When `super()` is used without arguments, Python tries to find two things:
    /// the nearest enclosing class and the first argument of the immediately enclosing function (typically self or cls).
    /// If either of these is missing, the call will fail at runtime with a `RuntimeError`.
    ///
    /// ## Examples
    /// ```python
    /// super()  # error: no enclosing class or function found
    ///
    /// def func():
    ///     super()  # error: no enclosing class or first argument exists
    ///
    /// class A:
    ///     f = super()  # error: no enclosing function to provide the first argument
    ///
    ///     def method(self):
    ///         def nested():
    ///             super()  # error: first argument does not exist in this nested function
    ///
    ///         lambda: super()  # error: first argument does not exist in this lambda
    ///
    ///         (super() for _ in range(10))  # error: argument is not available in generator expression
    ///
    ///         super()  # okay! both enclosing class and first argument are available
    /// ```
    ///
    /// ## References
    /// - [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)
    pub(crate) static UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS = {
        summary: "detects invalid `super()` calls where implicit arguments are unavailable.",
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
    /// ```python
    /// reveal_type(1)  # NameError: name 'reveal_type' is not defined
    /// ```
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
    /// ## Why is this bad?
    /// Accessing an unbound attribute will raise an `AttributeError` at runtime.
    /// An unresolved attribute is not guaranteed to exist from the type alone,
    /// so this could also indicate that the object is not of the type that the user expects.
    ///
    /// ## Examples
    /// ```python
    /// class A: ...
    ///
    /// A().foo  # AttributeError: 'A' object has no attribute 'foo'
    /// ```
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
    /// Importing a module that cannot be resolved will raise a `ModuleNotFoundError`
    /// at runtime.
    ///
    /// ## Examples
    /// ```python
    /// import foo  # ModuleNotFoundError: No module named 'foo'
    /// ```
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
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for binary expressions, comparisons, and unary expressions where
    /// the operands don't support the operator.
    ///
    /// ## Why is this bad?
    /// Attempting to use an unsupported operator will raise a `TypeError` at
    /// runtime.
    ///
    /// ## Examples
    /// ```python
    /// class A: ...
    ///
    /// A() + A()  # TypeError: unsupported operand type(s) for +: 'A' and 'A'
    /// ```
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
    /// ## Why is this bad?
    /// A `static_assert` call represents an explicit request from the user
    /// for the type checker to emit an error if the argument cannot be verified
    /// to evaluate to `True` in a boolean context.
    ///
    /// ## Examples
    /// ```python
    /// from ty_extensions import static_assert
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
    /// Checks for assignments to class variables from instances
    /// and assignments to instance variables from its class.
    ///
    /// ## Why is this bad?
    /// Incorrect assignments break the rules of the type system and
    /// weaken a type checker's ability to accurately reason about your code.
    ///
    /// ## Examples
    /// ```python
    /// class C:
    ///     class_var: ClassVar[int] = 1
    ///     instance_var: int
    ///
    /// C.class_var = 3  # okay
    /// C().class_var = 3  # error: Cannot assign to class variable
    ///
    /// C().instance_var = 3  # okay
    /// C.instance_var = 3  # error: Cannot assign to instance variable
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

    pub(super) fn extend_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
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
    let Some(builder) = context.report_lint(&INDEX_OUT_OF_BOUNDS, node) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Index {index} is out of bounds for {kind} `{}` with length {length}",
        tuple_ty.display(context.db())
    ));
}

/// Emit a diagnostic declaring that a type does not support subscripting.
pub(super) fn report_non_subscriptable(
    context: &InferContext,
    node: AnyNodeRef,
    non_subscriptable_ty: Type,
    method: &str,
) {
    let Some(builder) = context.report_lint(&NON_SUBSCRIPTABLE, node) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Cannot subscript object of type `{}` with no `{method}` method",
        non_subscriptable_ty.display(context.db())
    ));
}

pub(super) fn report_slice_step_size_zero(context: &InferContext, node: AnyNodeRef) {
    let Some(builder) = context.report_lint(&ZERO_STEPSIZE_IN_SLICE, node) else {
        return;
    };
    builder.into_diagnostic("Slice step size can not be zero");
}

fn report_invalid_assignment_with_message(
    context: &InferContext,
    node: AnyNodeRef,
    target_ty: Type,
    message: std::fmt::Arguments,
) {
    let Some(builder) = context.report_lint(&INVALID_ASSIGNMENT, node) else {
        return;
    };
    match target_ty {
        Type::ClassLiteral(class) => {
            let mut diag = builder.into_diagnostic(format_args!(
                "Implicit shadowing of class `{}`",
                class.name(context.db()),
            ));
            diag.info("Annotate to make it explicit if this is intentional");
        }
        Type::FunctionLiteral(function) => {
            let mut diag = builder.into_diagnostic(format_args!(
                "Implicit shadowing of function `{}`",
                function.name(context.db()),
            ));
            diag.info("Annotate to make it explicit if this is intentional");
        }
        _ => {
            builder.into_diagnostic(message);
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
    let Some(builder) = context.report_lint(&INVALID_RETURN_TYPE, object_range) else {
        return;
    };

    let return_type_span = context.span(return_type_range);

    let mut diag = builder.into_diagnostic("Return type does not match returned value");
    diag.set_primary_message(format_args!(
        "expected `{expected_ty}`, found `{actual_ty}`",
        expected_ty = expected_ty.display(context.db()),
        actual_ty = actual_ty.display(context.db()),
    ));
    diag.annotate(
        Annotation::secondary(return_type_span).message(format_args!(
            "Expected `{expected_ty}` because of return type",
            expected_ty = expected_ty.display(context.db()),
        )),
    );
}

pub(super) fn report_invalid_generator_function_return_type(
    context: &InferContext,
    return_type_range: TextRange,
    inferred_return: KnownClass,
    expected_ty: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_RETURN_TYPE, return_type_range) else {
        return;
    };

    let mut diag = builder.into_diagnostic("Return type does not match returned value");
    let inferred_ty = inferred_return.display(context.db());
    diag.set_primary_message(format_args!(
        "expected `{expected_ty}`, found `{inferred_ty}`",
        expected_ty = expected_ty.display(context.db()),
    ));

    let (description, link) = if inferred_return == KnownClass::AsyncGeneratorType {
        (
            "an async generator function",
            "https://docs.python.org/3/glossary.html#term-asynchronous-generator",
        )
    } else {
        (
            "a generator function",
            "https://docs.python.org/3/glossary.html#term-generator",
        )
    };

    diag.info(format_args!(
        "Function is inferred as returning `{inferred_ty}` because it is {description}"
    ));
    diag.info(format_args!("See {link} for more details"));
}

pub(super) fn report_implicit_return_type(
    context: &InferContext,
    range: impl Ranged,
    expected_ty: Type,
    has_empty_body: bool,
    enclosing_class_of_method: Option<ClassLiteral>,
) {
    let Some(builder) = context.report_lint(&INVALID_RETURN_TYPE, range) else {
        return;
    };
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Function can implicitly return `None`, which is not assignable to return type `{}`",
        expected_ty.display(db)
    ));
    if !has_empty_body {
        return;
    }
    let Some(class) = enclosing_class_of_method else {
        return;
    };
    if class.iter_mro(db, None).contains(&ClassBase::Protocol) {
        diagnostic.info(
            "Only functions in stub files, methods on protocol classes, \
            or methods with `@abstractmethod` are permitted to have empty bodies",
        );
        diagnostic.info(format_args!(
            "Class `{}` has `typing.Protocol` in its MRO, but it is not a protocol class",
            class.name(db)
        ));

        let mut sub_diagnostic = SubDiagnostic::new(
            Severity::Info,
            "Only classes that directly inherit from `typing.Protocol` \
            or `typing_extensions.Protocol` are considered protocol classes",
        );
        sub_diagnostic.annotate(
            Annotation::primary(class.header_span(db)).message(format_args!(
                "`Protocol` not present in `{class}`'s immediate bases",
                class = class.name(db)
            )),
        );
        diagnostic.sub(sub_diagnostic);

        diagnostic.info("See https://typing.python.org/en/latest/spec/protocol.html#");
    }
}

pub(super) fn report_invalid_type_checking_constant(context: &InferContext, node: AnyNodeRef) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_CHECKING_CONSTANT, node) else {
        return;
    };
    builder.into_diagnostic(
        "The name TYPE_CHECKING is reserved for use as a flag; only False can be assigned to it",
    );
}

pub(super) fn report_possibly_unresolved_reference(
    context: &InferContext,
    expr_name_node: &ast::ExprName,
) {
    let Some(builder) = context.report_lint(&POSSIBLY_UNRESOLVED_REFERENCE, expr_name_node) else {
        return;
    };

    let ast::ExprName { id, .. } = expr_name_node;
    builder.into_diagnostic(format_args!("Name `{id}` used when possibly not defined"));
}

pub(super) fn report_possibly_unbound_attribute(
    context: &InferContext,
    target: &ast::ExprAttribute,
    attribute: &str,
    object_ty: Type,
) {
    let Some(builder) = context.report_lint(&POSSIBLY_UNBOUND_ATTRIBUTE, target) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Attribute `{attribute}` on type `{}` is possibly unbound",
        object_ty.display(context.db()),
    ));
}

pub(super) fn add_inferred_python_version_hint(db: &dyn Db, mut diagnostic: LintDiagnosticGuard) {
    let program = Program::get(db);
    let PythonVersionWithSource { version, source } = program.python_version_with_source(db);

    match source {
        crate::PythonVersionSource::Cli => {
            diagnostic.info(format_args!(
                "Python {version} was assumed when resolving types because it was specified on the command line",
            ));
        }
        crate::PythonVersionSource::File(path, range) => {
            if let Ok(file) = system_path_to_file(db.upcast(), &**path) {
                let mut sub_diagnostic = SubDiagnostic::new(
                    Severity::Info,
                    format_args!("Python {version} was assumed when resolving types"),
                );
                sub_diagnostic.annotate(
                    Annotation::primary(Span::from(file).with_optional_range(*range)).message(
                        format_args!("Python {version} assumed due to this configuration setting"),
                    ),
                );
                diagnostic.sub(sub_diagnostic);
            } else {
                diagnostic.info(format_args!(
                    "Python {version} was assumed when resolving types because of your configuration file(s)",
                ));
            }
        }
        crate::PythonVersionSource::Default => {
            diagnostic.info(format_args!(
                "Python {version} was assumed when resolving types \
                because it is the newest Python version supported by ty, \
                and neither a command-line argument nor a configuration setting was provided",
            ));
        }
    }
}

pub(super) fn report_unresolved_reference(context: &InferContext, expr_name_node: &ast::ExprName) {
    let Some(builder) = context.report_lint(&UNRESOLVED_REFERENCE, expr_name_node) else {
        return;
    };

    let ast::ExprName { id, .. } = expr_name_node;
    let mut diagnostic = builder.into_diagnostic(format_args!("Name `{id}` used when not defined"));
    if let Some(version_added_to_builtins) = version_builtin_was_added(id) {
        diagnostic.info(format_args!(
            "`{id}` was added as a builtin in Python 3.{version_added_to_builtins}"
        ));
        add_inferred_python_version_hint(context.db(), diagnostic);
    }
}

pub(super) fn report_invalid_exception_caught(context: &InferContext, node: &ast::Expr, ty: Type) {
    let Some(builder) = context.report_lint(&INVALID_EXCEPTION_CAUGHT, node) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Cannot catch object of type `{}` in an exception handler \
            (must be a `BaseException` subclass or a tuple of `BaseException` subclasses)",
        ty.display(context.db())
    ));
}

pub(crate) fn report_invalid_exception_raised(context: &InferContext, node: &ast::Expr, ty: Type) {
    let Some(builder) = context.report_lint(&INVALID_RAISE, node) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Cannot raise object of type `{}` (must be a `BaseException` subclass or instance)",
        ty.display(context.db())
    ));
}

pub(crate) fn report_invalid_exception_cause(context: &InferContext, node: &ast::Expr, ty: Type) {
    let Some(builder) = context.report_lint(&INVALID_RAISE, node) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Cannot use object of type `{}` as exception cause \
         (must be a `BaseException` subclass or instance or `None`)",
        ty.display(context.db())
    ));
}

pub(crate) fn report_base_with_incompatible_slots(context: &InferContext, node: &ast::Expr) {
    let Some(builder) = context.report_lint(&INCOMPATIBLE_SLOTS, node) else {
        return;
    };
    builder.into_diagnostic("Class base has incompatible `__slots__`");
}

pub(crate) fn report_invalid_arguments_to_annotated(
    db: &dyn Db,
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Special form `{}` expected at least 2 arguments \
         (one type and at least one metadata element)",
        KnownInstanceType::Annotated.repr(db)
    ));
}

pub(crate) fn report_bad_argument_to_get_protocol_members(
    context: &InferContext,
    call: &ast::ExprCall,
    class: ClassLiteral,
) {
    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call) else {
        return;
    };
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic("Invalid argument to `get_protocol_members`");
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");
    diagnostic.info("Only protocol classes can be passed to `get_protocol_members`");

    let mut class_def_diagnostic = SubDiagnostic::new(
        Severity::Info,
        format_args!(
            "`{}` is declared here, but it is not a protocol class:",
            class.name(db)
        ),
    );
    class_def_diagnostic.annotate(Annotation::primary(class.header_span(db)));
    diagnostic.sub(class_def_diagnostic);

    diagnostic.info(
        "A class is only a protocol class if it directly inherits \
            from `typing.Protocol` or `typing_extensions.Protocol`",
    );
    // TODO the typing spec isn't really designed as user-facing documentation,
    // but there isn't really any user-facing documentation that covers this specific issue well
    // (it's not described well in the CPython docs; and PEP-544 is a snapshot of a decision taken
    // years ago rather than up-to-date documentation). We should either write our own docs
    // describing this well or contribute to type-checker-agnostic docs somewhere and link to those.
    diagnostic.info("See https://typing.python.org/en/latest/spec/protocol.html#");
}

pub(crate) fn report_invalid_arguments_to_callable(
    db: &dyn Db,
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Special form `{}` expected exactly two arguments (parameter types and return type)",
        KnownInstanceType::Callable.repr(db)
    ));
}

pub(crate) fn add_type_expression_reference_link(mut diag: LintDiagnosticGuard) {
    diag.info("See the following page for a reference on valid type expressions:");
    diag.info(
        "https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions",
    );
}

pub(crate) fn report_runtime_check_against_non_runtime_checkable_protocol(
    context: &InferContext,
    call: &ast::ExprCall,
    protocol: ProtocolClassLiteral,
    function: KnownFunction,
) {
    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call) else {
        return;
    };
    let db = context.db();
    let class_name = protocol.name(db);
    let function_name: &'static str = function.into();
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Class `{class_name}` cannot be used as the second argument to `{function_name}`",
    ));
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");

    let mut class_def_diagnostic = SubDiagnostic::new(
        Severity::Info,
        format_args!(
            "`{class_name}` is declared as a protocol class, \
                but it is not declared as runtime-checkable"
        ),
    );
    class_def_diagnostic.annotate(
        Annotation::primary(protocol.header_span(db))
            .message(format_args!("`{class_name}` declared here")),
    );
    diagnostic.sub(class_def_diagnostic);

    diagnostic.info(format_args!(
        "A protocol class can only be used in `{function_name}` checks if it is decorated \
            with `@typing.runtime_checkable` or `@typing_extensions.runtime_checkable`"
    ));
    diagnostic.info("See https://docs.python.org/3/library/typing.html#typing.runtime_checkable");
}

pub(crate) fn report_attempted_protocol_instantiation(
    context: &InferContext,
    call: &ast::ExprCall,
    protocol: ProtocolClassLiteral,
) {
    let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, call) else {
        return;
    };
    let db = context.db();
    let class_name = protocol.name(db);
    let mut diagnostic =
        builder.into_diagnostic(format_args!("Cannot instantiate class `{class_name}`",));
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");

    let mut class_def_diagnostic = SubDiagnostic::new(
        Severity::Info,
        format_args!("Protocol classes cannot be instantiated"),
    );
    class_def_diagnostic.annotate(
        Annotation::primary(protocol.header_span(db))
            .message(format_args!("`{class_name}` declared as a protocol here")),
    );
    diagnostic.sub(class_def_diagnostic);
}

pub(crate) fn report_duplicate_bases(
    context: &InferContext,
    class: ClassLiteral,
    duplicate_base_error: &DuplicateBaseError,
    bases_list: &[ast::Expr],
) {
    let db = context.db();

    let Some(builder) = context.report_lint(&DUPLICATE_BASE, class.header_range(db)) else {
        return;
    };

    let DuplicateBaseError {
        duplicate_base,
        first_index,
        later_indices,
    } = duplicate_base_error;

    let duplicate_name = duplicate_base.name(db);

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Duplicate base class `{duplicate_name}`",));

    let mut sub_diagnostic = SubDiagnostic::new(
        Severity::Info,
        format_args!(
            "The definition of class `{}` will raise `TypeError` at runtime",
            class.name(db)
        ),
    );
    sub_diagnostic.annotate(
        Annotation::secondary(context.span(&bases_list[*first_index])).message(format_args!(
            "Class `{duplicate_name}` first included in bases list here"
        )),
    );
    for index in later_indices {
        sub_diagnostic.annotate(
            Annotation::primary(context.span(&bases_list[*index]))
                .message(format_args!("Class `{duplicate_name}` later repeated here")),
        );
    }

    diagnostic.sub(sub_diagnostic);
}

pub(crate) fn report_invalid_or_unsupported_base(
    context: &InferContext,
    base_node: &ast::Expr,
    base_type: Type,
    class: ClassLiteral,
) {
    let db = context.db();
    let instance_of_type = KnownClass::Type.to_instance(db);

    if base_type.is_assignable_to(db, instance_of_type) {
        report_unsupported_base(context, base_node, base_type, class);
        return;
    }

    let tuple_of_types = KnownClass::Tuple.to_specialized_instance(db, [instance_of_type]);

    let explain_mro_entries = |diagnostic: &mut LintDiagnosticGuard| {
        diagnostic.info(
            "An instance type is only a valid class base \
            if it has a valid `__mro_entries__` method",
        );
    };

    match base_type.try_call_dunder(
        db,
        "__mro_entries__",
        CallArgumentTypes::positional([tuple_of_types]),
    ) {
        Ok(ret) => {
            if ret.return_type(db).is_assignable_to(db, tuple_of_types) {
                report_unsupported_base(context, base_node, base_type, class);
            } else {
                let Some(mut diagnostic) =
                    report_invalid_base(context, base_node, base_type, class)
                else {
                    return;
                };
                explain_mro_entries(&mut diagnostic);
                diagnostic.info(format_args!(
                    "Type `{}` has an `__mro_entries__` method, but it does not return a tuple of types",
                    base_type.display(db)
                ));
            }
        }
        Err(mro_entries_call_error) => {
            let Some(mut diagnostic) = report_invalid_base(context, base_node, base_type, class)
            else {
                return;
            };

            match mro_entries_call_error {
                CallDunderError::MethodNotAvailable => {}
                CallDunderError::PossiblyUnbound(_) => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` has an `__mro_entries__` attribute, but it is possibly unbound",
                        base_type.display(db)
                    ));
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, _) => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` has an `__mro_entries__` attribute, but it is not callable",
                        base_type.display(db)
                    ));
                }
                CallDunderError::CallError(CallErrorKind::BindingError, _) => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` has an `__mro_entries__` method, \
                        but it cannot be called with the expected arguments",
                        base_type.display(db)
                    ));
                    diagnostic.info(
                        "Expected a signature at least as permissive as \
                        `def __mro_entries__(self, bases: tuple[type, ...], /) -> tuple[type, ...]`"
                    );
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _) => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` has an `__mro_entries__` method, \
                        but it may not be callable",
                        base_type.display(db)
                    ));
                }
            }
        }
    }
}

fn report_unsupported_base(
    context: &InferContext,
    base_node: &ast::Expr,
    base_type: Type,
    class: ClassLiteral,
) {
    let Some(builder) = context.report_lint(&UNSUPPORTED_BASE, base_node) else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Unsupported class base with type `{}`",
        base_type.display(context.db())
    ));
    diagnostic.info(format_args!(
        "ty cannot resolve a consistent MRO for class `{}` due to this base",
        class.name(context.db())
    ));
    diagnostic.info("Only class objects or `Any` are supported as class bases");
}

fn report_invalid_base<'ctx, 'db>(
    context: &'ctx InferContext<'db>,
    base_node: &ast::Expr,
    base_type: Type<'db>,
    class: ClassLiteral<'db>,
) -> Option<LintDiagnosticGuard<'ctx, 'db>> {
    let builder = context.report_lint(&INVALID_BASE, base_node)?;
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Invalid class base with type `{}`",
        base_type.display(context.db())
    ));
    diagnostic.info(format_args!(
        "Definition of class `{}` will raise `TypeError` at runtime",
        class.name(context.db())
    ));
    Some(diagnostic)
}
