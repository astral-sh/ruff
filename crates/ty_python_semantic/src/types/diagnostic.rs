use super::call::CallErrorKind;
use super::context::InferContext;
use super::mro::DuplicateBaseError;
use super::{
    CallArguments, CallDunderError, ClassBase, ClassLiteral, KnownClass,
    add_inferred_python_version_hint_to_diagnostic,
};
use crate::diagnostic::did_you_mean;
use crate::diagnostic::format_enumeration;
use crate::lint::{Level, LintRegistryBuilder, LintStatus};
use crate::place::Place;
use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::semantic_index::place::{PlaceTable, ScopedPlaceId};
use crate::semantic_index::{global_scope, place_table, use_def_map};
use crate::suppression::FileSuppressionId;
use crate::types::call::CallError;
use crate::types::class::{CodeGeneratorKind, DisjointBase, DisjointBaseKind, MethodDecorator};
use crate::types::function::{FunctionDecorators, FunctionType, KnownFunction, OverloadLiteral};
use crate::types::infer::UnsupportedComparisonError;
use crate::types::overrides::MethodKind;
use crate::types::string_annotation::{
    BYTE_STRING_TYPE_ANNOTATION, ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION, FSTRING_TYPE_ANNOTATION,
    IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION, INVALID_SYNTAX_IN_FORWARD_ANNOTATION,
    RAW_STRING_TYPE_ANNOTATION,
};
use crate::types::tuple::TupleSpec;
use crate::types::typed_dict::TypedDictSchema;
use crate::types::{
    BoundTypeVarInstance, ClassType, DynamicType, LintDiagnosticGuard, Protocol,
    ProtocolInstanceType, SpecialFormType, SubclassOfInner, Type, TypeContext, binding_type,
    protocol_class::ProtocolClass,
};
use crate::types::{DataclassFlags, KnownInstanceType, MemberLookupPolicy, TypeVarInstance};
use crate::{Db, DisplaySettings, FxIndexMap, Program, declare_lint};
use itertools::Itertools;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
};
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::name::Name;
use ruff_python_ast::token::parentheses_iterator;
use ruff_python_ast::{self as ast, AnyNodeRef, PythonVersion, StringFlags};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use std::fmt::{self, Formatter};
use ty_module_resolver::{Module, ModuleName};

/// Registers all known type check lints.
pub(crate) fn register_lints(registry: &mut LintRegistryBuilder) {
    registry.register_lint(&AMBIGUOUS_PROTOCOL_MEMBER);
    registry.register_lint(&CALL_NON_CALLABLE);
    registry.register_lint(&CALL_TOP_CALLABLE);
    registry.register_lint(&POSSIBLY_MISSING_IMPLICIT_CALL);
    registry.register_lint(&CONFLICTING_ARGUMENT_FORMS);
    registry.register_lint(&CONFLICTING_DECLARATIONS);
    registry.register_lint(&CONFLICTING_METACLASS);
    registry.register_lint(&CYCLIC_CLASS_DEFINITION);
    registry.register_lint(&CYCLIC_TYPE_ALIAS_DEFINITION);
    registry.register_lint(&DEPRECATED);
    registry.register_lint(&DIVISION_BY_ZERO);
    registry.register_lint(&DUPLICATE_BASE);
    registry.register_lint(&DUPLICATE_KW_ONLY);
    registry.register_lint(&INSTANCE_LAYOUT_CONFLICT);
    registry.register_lint(&INCONSISTENT_MRO);
    registry.register_lint(&INDEX_OUT_OF_BOUNDS);
    registry.register_lint(&INVALID_KEY);
    registry.register_lint(&INVALID_ARGUMENT_TYPE);
    registry.register_lint(&INVALID_RETURN_TYPE);
    registry.register_lint(&INVALID_ASSIGNMENT);
    registry.register_lint(&INVALID_AWAIT);
    registry.register_lint(&INVALID_BASE);
    registry.register_lint(&INVALID_CONTEXT_MANAGER);
    registry.register_lint(&INVALID_DECLARATION);
    registry.register_lint(&INVALID_EXCEPTION_CAUGHT);
    registry.register_lint(&INVALID_GENERIC_CLASS);
    registry.register_lint(&INVALID_LEGACY_TYPE_VARIABLE);
    registry.register_lint(&INVALID_PARAMSPEC);
    registry.register_lint(&INVALID_TYPE_ALIAS_TYPE);
    registry.register_lint(&INVALID_NEWTYPE);
    registry.register_lint(&INVALID_METACLASS);
    registry.register_lint(&INVALID_OVERLOAD);
    registry.register_lint(&USELESS_OVERLOAD_BODY);
    registry.register_lint(&INVALID_PARAMETER_DEFAULT);
    registry.register_lint(&INVALID_PROTOCOL);
    registry.register_lint(&INVALID_NAMED_TUPLE);
    registry.register_lint(&INVALID_RAISE);
    registry.register_lint(&INVALID_SUPER_ARGUMENT);
    registry.register_lint(&INVALID_TYPE_ARGUMENTS);
    registry.register_lint(&INVALID_TYPE_CHECKING_CONSTANT);
    registry.register_lint(&INVALID_TYPE_FORM);
    registry.register_lint(&INVALID_TYPE_GUARD_DEFINITION);
    registry.register_lint(&INVALID_TYPE_GUARD_CALL);
    registry.register_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS);
    registry.register_lint(&MISSING_ARGUMENT);
    registry.register_lint(&NO_MATCHING_OVERLOAD);
    registry.register_lint(&NOT_SUBSCRIPTABLE);
    registry.register_lint(&NOT_ITERABLE);
    registry.register_lint(&UNSUPPORTED_BOOL_CONVERSION);
    registry.register_lint(&PARAMETER_ALREADY_ASSIGNED);
    registry.register_lint(&POSSIBLY_MISSING_ATTRIBUTE);
    registry.register_lint(&POSSIBLY_MISSING_IMPORT);
    registry.register_lint(&POSSIBLY_UNRESOLVED_REFERENCE);
    registry.register_lint(&SUBCLASS_OF_FINAL_CLASS);
    registry.register_lint(&OVERRIDE_OF_FINAL_METHOD);
    registry.register_lint(&TYPE_ASSERTION_FAILURE);
    registry.register_lint(&TOO_MANY_POSITIONAL_ARGUMENTS);
    registry.register_lint(&UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS);
    registry.register_lint(&UNDEFINED_REVEAL);
    registry.register_lint(&UNKNOWN_ARGUMENT);
    registry.register_lint(&POSITIONAL_ONLY_PARAMETER_AS_KWARG);
    registry.register_lint(&UNRESOLVED_ATTRIBUTE);
    registry.register_lint(&UNRESOLVED_IMPORT);
    registry.register_lint(&UNRESOLVED_REFERENCE);
    registry.register_lint(&UNSUPPORTED_BASE);
    registry.register_lint(&UNSUPPORTED_OPERATOR);
    registry.register_lint(&ZERO_STEPSIZE_IN_SLICE);
    registry.register_lint(&STATIC_ASSERT_ERROR);
    registry.register_lint(&INVALID_ATTRIBUTE_ACCESS);
    registry.register_lint(&REDUNDANT_CAST);
    registry.register_lint(&UNRESOLVED_GLOBAL);
    registry.register_lint(&MISSING_TYPED_DICT_KEY);
    registry.register_lint(&INVALID_METHOD_OVERRIDE);
    registry.register_lint(&INVALID_EXPLICIT_OVERRIDE);
    registry.register_lint(&SUPER_CALL_IN_NAMED_TUPLE_METHOD);
    registry.register_lint(&INVALID_FROZEN_DATACLASS_SUBCLASS);

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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to objects typed as `Top[Callable[..., T]]` (the infinite union of all
    /// callable types with return type `T`).
    ///
    /// ## Why is this bad?
    /// When an object is narrowed to `Top[Callable[..., object]]` (e.g., via `callable(x)` or
    /// `isinstance(x, Callable)`), we know the object is callable, but we don't know its
    /// precise signature. This type represents the set of all possible callable types
    /// (including, e.g., functions that take no arguments and functions that require arguments),
    /// so no specific set of arguments can be guaranteed to be valid.
    ///
    /// ## Examples
    /// ```python
    /// def f(x: object):
    ///     if callable(x):
    ///         x()  # error: We know `x` is callable, but not what arguments it accepts
    /// ```
    pub(crate) static CALL_TOP_CALLABLE = {
        summary: "detects calls to the top callable type",
        status: LintStatus::stable("0.0.7"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for implicit calls to possibly missing methods.
    ///
    /// ## Why is this bad?
    /// Expressions such as `x[y]` and `x * y` call methods
    /// under the hood (`__getitem__` and `__mul__` respectively).
    /// Calling a missing method will raise an `AttributeError` at runtime.
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
    pub(crate) static POSSIBLY_MISSING_IMPLICIT_CALL = {
        summary: "detects implicit calls to possibly missing methods",
        status: LintStatus::stable("0.0.1-alpha.22"),
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
    /// from ty_extensions import is_singleton
    ///
    /// if flag:
    ///     f = repr  # Expects a value
    /// else:
    ///     f = is_singleton  # Expects a type form
    ///
    /// f(int)  # error
    /// ```
    pub(crate) static CONFLICTING_ARGUMENT_FORMS = {
        summary: "detects when an argument is used as both a value and a type form in a call",
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for type alias definitions that (directly or mutually) refer to themselves.
    ///
    /// ## Why is it bad?
    /// Although it is permitted to define a recursive type alias, it is not meaningful
    /// to have a type alias whose expansion can only result in itself, and is therefore not allowed.
    ///
    /// ## Examples
    /// ```python
    /// type Itself = Itself
    ///
    /// type A = B
    /// type B = A
    /// ```
    pub(crate) static CYCLIC_TYPE_ALIAS_DEFINITION = {
        summary: "detects cyclic type alias definitions",
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
    /// ## Rule status
    /// This rule is currently disabled by default because of the number of
    /// false positives it can produce.
    ///
    /// ## Examples
    /// ```python
    /// 5 / 0
    /// ```
    pub(crate) static DIVISION_BY_ZERO = {
        summary: "detects division by zero",
        status: LintStatus::preview("0.0.1-alpha.1"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for uses of deprecated items
    ///
    /// ## Why is this bad?
    /// Deprecated items should no longer be used.
    ///
    /// ## Examples
    /// ```python
    /// @warnings.deprecated("use new_func instead")
    /// def old_func(): ...
    ///
    /// old_func()  # emits [deprecated] diagnostic
    /// ```
    pub(crate) static DEPRECATED = {
        summary: "detects uses of deprecated items",
        status: LintStatus::stable("0.0.1-alpha.16"),
        default_level: Level::Warn,
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for dataclass definitions with more than one field
    /// annotated with `KW_ONLY`.
    ///
    /// ## Why is this bad?
    /// `dataclasses.KW_ONLY` is a special marker used to
    /// emulate the `*` syntax in normal signatures.
    /// It can only be used once per dataclass.
    ///
    /// Attempting to annotate two different fields with
    /// it will lead to a runtime error.
    ///
    /// ## Examples
    /// ```python
    /// from dataclasses import dataclass, KW_ONLY
    ///
    /// @dataclass
    /// class A:  # Crash at runtime
    ///     b: int
    ///     _1: KW_ONLY
    ///     c: str
    ///     _2: KW_ONLY
    ///     d: bytes
    /// ```
    pub(crate) static DUPLICATE_KW_ONLY = {
        summary: "detects dataclass definitions with more than one usage of `KW_ONLY`",
        status: LintStatus::stable("0.0.1-alpha.12"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for classes definitions which will fail at runtime due to
    /// "instance memory layout conflicts".
    ///
    /// This error is usually caused by attempting to combine multiple classes
    /// that define non-empty `__slots__` in a class's [Method Resolution Order]
    /// (MRO), or by attempting to combine multiple builtin classes in a class's
    /// MRO.
    ///
    /// ## Why is this bad?
    /// Inheriting from bases with conflicting instance memory layouts
    /// will lead to a `TypeError` at runtime.
    ///
    /// An instance memory layout conflict occurs when CPython cannot determine
    /// the memory layout instances of a class should have, because the instance
    /// memory layout of one of its bases conflicts with the instance memory layout
    /// of one or more of its other bases.
    ///
    /// For example, if a Python class defines non-empty `__slots__`, this will
    /// impact the memory layout of instances of that class. Multiple inheritance
    /// from more than one different class defining non-empty `__slots__` is not
    /// allowed:
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
    /// An instance layout conflict can also be caused by attempting to use
    /// multiple inheritance with two builtin classes, due to the way that these
    /// classes are implemented in a CPython C extension:
    ///
    /// ```python
    /// class A(int, float): ...  # TypeError: multiple bases have instance lay-out conflict
    /// ```
    ///
    /// Note that pure-Python classes with no `__slots__`, or pure-Python classes
    /// with empty `__slots__`, are always compatible:
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
    /// ## Known problems
    /// Classes that have "dynamic" definitions of `__slots__` (definitions do not consist
    /// of string literals, or tuples of string literals) are not currently considered disjoint
    /// bases by ty.
    ///
    /// Additionally, this check is not exhaustive: many C extensions (including several in
    /// the standard library) define classes that use extended memory layouts and thus cannot
    /// coexist in a single MRO. Since it is currently not possible to represent this fact in
    /// stub files, having a full knowledge of these classes is also impossible. When it comes
    /// to classes that do not define `__slots__` at the Python level, therefore, ty, currently
    /// only hard-codes a number of cases where it knows that a class will produce instances with
    /// an atypical memory layout.
    ///
    /// ## Further reading
    /// - [CPython documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
    /// - [CPython documentation: Method Resolution Order](https://docs.python.org/3/glossary.html#term-method-resolution-order)
    ///
    /// [Method Resolution Order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(crate) static INSTANCE_LAYOUT_CONFLICT = {
        summary: "detects class definitions that raise `TypeError` due to instance layout conflict",
        status: LintStatus::stable("0.0.1-alpha.12"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for protocol classes that will raise `TypeError` at runtime.
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

// Added in #17750.
declare_lint! {
    /// ## What it does
    /// Checks for protocol classes with members that will lead to ambiguous interfaces.
    ///
    /// ## Why is this bad?
    /// Assigning to an undeclared variable in a protocol class leads to an ambiguous
    /// interface which may lead to the type checker inferring unexpected things. It's
    /// recommended to ensure that all members of a protocol class are explicitly declared.
    ///
    /// ## Examples
    ///
    /// ```py
    /// from typing import Protocol
    ///
    /// class BaseProto(Protocol):
    ///     a: int                               # fine (explicitly declared as `int`)
    ///     def method_member(self) -> int: ...  # fine: a method definition using `def` is considered a declaration
    ///     c = "some variable"                  # error: no explicit declaration, leading to ambiguity
    ///     b = method_member                    # error: no explicit declaration, leading to ambiguity
    ///
    ///     # error: this creates implicit assignments of `d` and `e` in the protocol class body.
    ///     # Were they really meant to be considered protocol members?
    ///     for d, e in enumerate(range(42)):
    ///         pass
    ///
    /// class SubProto(BaseProto, Protocol):
    ///     a = 42  # fine (declared in superclass)
    /// ```
    pub(crate) static AMBIGUOUS_PROTOCOL_MEMBER = {
        summary: "detects protocol classes with ambiguous interfaces",
        status: LintStatus::stable("0.0.1-alpha.20"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for invalidly defined `NamedTuple` classes.
    ///
    /// ## Why is this bad?
    /// An invalidly defined `NamedTuple` class may lead to the type checker
    /// drawing incorrect conclusions. It may also lead to `TypeError`s or
    /// `AttributeError`s at runtime.
    ///
    /// ## Examples
    /// A class definition cannot combine `NamedTuple` with other base classes
    /// in multiple inheritance; doing so raises a `TypeError` at runtime. The sole
    /// exception to this rule is `Generic[]`, which can be used alongside `NamedTuple`
    /// in a class's bases list.
    ///
    /// ```pycon
    /// >>> from typing import NamedTuple
    /// >>> class Foo(NamedTuple, object): ...
    /// TypeError: can only inherit from a NamedTuple type and Generic
    /// ```
    ///
    /// Further, `NamedTuple` field names cannot start with an underscore:
    ///
    /// ```pycon
    /// >>> from typing import NamedTuple
    /// >>> class Foo(NamedTuple):
    /// ...     _bar: int
    /// ValueError: Field names cannot start with an underscore: '_bar'
    /// ```
    ///
    /// `NamedTuple` classes also have certain synthesized attributes (like `_asdict`, `_make`,
    /// `_replace`, etc.) that cannot be overwritten. Attempting to assign to these attributes
    /// without a type annotation will raise an `AttributeError` at runtime.
    ///
    /// ```pycon
    /// >>> from typing import NamedTuple
    /// >>> class Foo(NamedTuple):
    /// ...     x: int
    /// ...     _asdict = 42
    /// AttributeError: Cannot overwrite NamedTuple attribute _asdict
    /// ```
    pub(crate) static INVALID_NAMED_TUPLE = {
        summary: "detects invalid `NamedTuple` class definitions",
        status: LintStatus::stable("0.0.1-alpha.19"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

// Added in #19763.
declare_lint! {
    /// ## What it does
    /// Checks for subscript accesses with invalid keys and `TypedDict` construction with an
    /// unknown key.
    ///
    /// ## Why is this bad?
    /// Subscripting with an invalid key will raise a `KeyError` at runtime.
    ///
    /// Creating a `TypedDict` with an unknown key is likely a mistake; if the `TypedDict` is
    /// `closed=true` it also violates the expectations of the type.
    ///
    /// ## Examples
    /// ```python
    /// from typing import TypedDict
    ///
    /// class Person(TypedDict):
    ///     name: str
    ///     age: int
    ///
    /// alice = Person(name="Alice", age=30)
    /// alice["height"]  # KeyError: 'height'
    ///
    /// bob: Person = { "name": "Bob", "age": 30 }  # typo!
    ///
    /// carol = Person(name="Carol", age=25)  # typo!
    /// ```
    pub(crate) static INVALID_KEY = {
        summary: "detects invalid subscript accesses or TypedDict literal keys",
        status: LintStatus::stable("0.0.1-alpha.17"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for `await` being used with types that are not [Awaitable].
    ///
    /// ## Why is this bad?
    /// Such expressions will lead to `TypeError` being raised at runtime.
    ///
    /// ## Examples
    /// ```python
    /// import asyncio
    ///
    /// class InvalidAwait:
    ///     def __await__(self) -> int:
    ///         return 5
    ///
    /// async def main() -> None:
    ///     await InvalidAwait()  # error: [invalid-await]
    ///     await 42  # error: [invalid-await]
    ///
    /// asyncio.run(main())
    /// ```
    ///
    /// [Awaitable]: https://docs.python.org/3/library/collections.abc.html#collections.abc.Awaitable
    pub(crate) static INVALID_AWAIT = {
        summary: "detects awaiting on types that don't support it",
        status: LintStatus::stable("0.0.1-alpha.19"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.7"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for exception handlers that catch non-exception classes.
    ///
    /// ## Why is this bad?
    /// Catching classes that do not inherit from `BaseException` will raise a `TypeError` at runtime.
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for the creation of invalid generic classes
    ///
    /// ## Why is this bad?
    /// There are several requirements that you must follow when defining a generic class.
    /// Many of these result in `TypeError` being raised at runtime if they are violated.
    ///
    /// ## Examples
    /// ```python
    /// from typing_extensions import Generic, TypeVar
    ///
    /// T = TypeVar("T")
    /// U = TypeVar("U", default=int)
    ///
    /// # error: class uses both PEP-695 syntax and legacy syntax
    /// class C[U](Generic[T]): ...
    ///
    /// # error: type parameter with default comes before type parameter without default
    /// class D(Generic[U, T]): ...
    /// ```
    ///
    /// ## References
    /// - [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)
    pub(crate) static INVALID_GENERIC_CLASS = {
        summary: "detects invalid generic classes",
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for the creation of invalid `ParamSpec`s
    ///
    /// ## Why is this bad?
    /// There are several requirements that you must follow when creating a `ParamSpec`.
    ///
    /// ## Examples
    /// ```python
    /// from typing import ParamSpec
    ///
    /// P1 = ParamSpec("P1")  # okay
    /// P2 = ParamSpec("S2")  # error: ParamSpec name must match the variable it's assigned to
    /// ```
    ///
    /// ## References
    /// - [Typing spec: ParamSpec](https://typing.python.org/en/latest/spec/generics.html#paramspec)
    pub(crate) static INVALID_PARAMSPEC = {
        summary: "detects invalid ParamSpec usage",
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.6"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for the creation of invalid `NewType`s
    ///
    /// ## Why is this bad?
    /// There are several requirements that you must follow when creating a `NewType`.
    ///
    /// ## Examples
    /// ```python
    /// from typing import NewType
    ///
    /// def get_name() -> str: ...
    ///
    /// Foo = NewType("Foo", int)        # okay
    /// Bar = NewType(get_name(), int)   # error: The first argument to `NewType` must be a string literal
    /// Baz = NewType("Baz", int | str)  # error: invalid base for `typing.NewType`
    /// ```
    pub(crate) static INVALID_NEWTYPE = {
        summary: "detects invalid NewType definitions",
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for various `@overload`-decorated functions that have non-stub bodies.
    ///
    /// ## Why is this bad?
    /// Functions decorated with `@overload` are ignored at runtime; they are overridden
    /// by the implementation function that follows the series of overloads. While it is
    /// not illegal to provide a body for an `@overload`-decorated function, it may indicate
    /// a misunderstanding of how the `@overload` decorator works.
    ///
    /// ## Example
    ///
    /// ```py
    /// from typing import overload
    ///
    /// @overload
    /// def foo(x: int) -> int:
    ///     return x + 1  # will never be executed
    ///
    /// @overload
    /// def foo(x: str) -> str:
    ///     return "Oh no, got a string"  # will never be executed
    ///
    /// def foo(x: int | str) -> int | str:
    ///     raise Exception("unexpected type encountered")
    /// ```
    ///
    /// Use instead:
    ///
    /// ```py
    /// from typing import assert_never, overload
    ///
    /// @overload
    /// def foo(x: int) -> int: ...
    ///
    /// @overload
    /// def foo(x: str) -> str: ...
    ///
    /// def foo(x: int | str) -> int | str:
    ///     if isinstance(x, int):
    ///         return x + 1
    ///     elif isinstance(x, str):
    ///         return "Oh no, got a string"
    ///     else:
    ///         assert_never(x)
    /// ```
    ///
    /// ## References
    /// - [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)
    pub(crate) static USELESS_OVERLOAD_BODY = {
        summary: "detects `@overload`-decorated functions with non-stub bodies",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Warn,
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for type guard functions without
    /// a first non-self-like non-keyword-only non-variadic parameter.
    ///
    /// ## Why is this bad?
    /// Type narrowing functions must accept at least one positional argument
    /// (non-static methods must accept another in addition to `self`/`cls`).
    ///
    /// Extra parameters/arguments are allowed but do not affect narrowing.
    ///
    /// ## Examples
    /// ```python
    /// from typing import TypeIs
    ///
    /// def f() -> TypeIs[int]: ...  # Error, no parameter
    /// def f(*, v: object) -> TypeIs[int]: ...  # Error, no positional arguments allowed
    /// def f(*args: object) -> TypeIs[int]: ... # Error, expect variadic arguments
    /// class C:
    ///     def f(self) -> TypeIs[int]: ...  # Error, only positional argument expected is `self`
    /// ```
    pub(crate) static INVALID_TYPE_GUARD_DEFINITION = {
        summary: "detects malformed type guard functions",
        status: LintStatus::stable("0.0.1-alpha.11"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for type guard function calls without a valid target.
    ///
    /// ## Why is this bad?
    /// The first non-keyword non-variadic argument to a type guard function
    /// is its target and must map to a symbol.
    ///
    /// Starred (`is_str(*a)`), literal (`is_str(42)`) and other non-symbol-like
    /// expressions are invalid as narrowing targets.
    ///
    /// ## Examples
    /// ```python
    /// from typing import TypeIs
    ///
    /// def f(v: object) -> TypeIs[int]: ...
    ///
    /// f()  # Error
    /// f(*a)  # Error
    /// f(10)  # Error
    /// ```
    pub(crate) static INVALID_TYPE_GUARD_CALL = {
        summary: "detects type guard function calls that has no narrowing effect",
        status: LintStatus::stable("0.0.1-alpha.11"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
    pub(crate) static NOT_SUBSCRIPTABLE = {
        summary: "detects subscripting objects that do not support subscripting",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for invalid type arguments in explicit type specialization.
    ///
    /// ## Why is this bad?
    /// Providing the wrong number of type arguments or type arguments that don't
    /// satisfy the type variable's bounds or constraints will lead to incorrect
    /// type inference and may indicate a misunderstanding of the generic type's
    /// interface.
    ///
    /// ## Examples
    ///
    /// Using legacy type variables:
    /// ```python
    /// from typing import Generic, TypeVar
    ///
    /// T1 = TypeVar('T1', int, str)
    /// T2 = TypeVar('T2', bound=int)
    ///
    /// class Foo1(Generic[T1]): ...
    /// class Foo2(Generic[T2]): ...
    ///
    /// Foo1[bytes]  # error: bytes does not satisfy T1's constraints
    /// Foo2[str]  # error: str does not satisfy T2's bound
    /// ```
    ///
    /// Using PEP 695 type variables:
    /// ```python
    /// class Foo[T]: ...
    /// class Bar[T, U]: ...
    ///
    /// Foo[int, str]  # error: too many arguments
    /// Bar[int]  # error: too few arguments
    /// ```
    pub(crate) static INVALID_TYPE_ARGUMENTS = {
        summary: "detects invalid type arguments in generic specialization",
        status: LintStatus::stable("0.0.1-alpha.29"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for possibly missing attributes.
    ///
    /// ## Why is this bad?
    /// Attempting to access a missing attribute will raise an `AttributeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// class A:
    ///     if b:
    ///         c = 0
    ///
    /// A.c  # AttributeError: type object 'A' has no attribute 'c'
    /// ```
    pub(crate) static POSSIBLY_MISSING_ATTRIBUTE = {
        summary: "detects references to possibly missing attributes",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for imports of symbols that may be missing.
    ///
    /// ## Why is this bad?
    /// Importing a missing module or name will raise a `ModuleNotFoundError`
    /// or `ImportError` at runtime.
    ///
    /// ## Rule status
    /// This rule is currently disabled by default because of the number of
    /// false positives it can produce.
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
    pub(crate) static POSSIBLY_MISSING_IMPORT = {
        summary: "detects possibly missing imports",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for references to names that are possibly not defined.
    ///
    /// ## Why is this bad?
    /// Using an undefined variable will raise a `NameError` at runtime.
    ///
    /// ## Rule status
    /// This rule is currently disabled by default because of the number of
    /// false positives it can produce.
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for methods on subclasses that override superclass methods decorated with `@final`.
    ///
    /// ## Why is this bad?
    /// Decorating a method with `@final` declares to the type checker that it should not be
    /// overridden on any subclass.
    ///
    /// ## Example
    ///
    /// ```python
    /// from typing import final
    ///
    /// class A:
    ///     @final
    ///     def foo(self): ...
    ///
    /// class B(A):
    ///     def foo(self): ...  # Error raised here
    /// ```
    pub(crate) static OVERRIDE_OF_FINAL_METHOD = {
        summary: "detects overrides of final methods",
        status: LintStatus::stable("0.0.1-alpha.29"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for methods that are decorated with `@override` but do not override any method in a superclass.
    ///
    /// ## Why is this bad?
    /// Decorating a method with `@override` declares to the type checker that the intention is that it should
    /// override a method from a superclass.
    ///
    /// ## Example
    ///
    /// ```python
    /// from typing import override
    ///
    /// class A:
    ///     @override
    ///     def foo(self): ...  # Error raised here
    ///
    /// class B(A):
    ///     @override
    ///     def ffooo(self): ...  # Error raised here
    ///
    /// class C:
    ///     @override
    ///     def __repr__(self): ...  # fine: overrides `object.__repr__`
    ///
    /// class D(A):
    ///     @override
    ///     def foo(self): ...  # fine: overrides `A.foo`
    /// ```
    pub(crate) static INVALID_EXPLICIT_OVERRIDE = {
        summary: "detects methods that are decorated with `@override` but do not override any method in a superclass",
        status: LintStatus::stable("0.0.1-alpha.28"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to `super()` inside methods of `NamedTuple` classes.
    ///
    /// ## Why is this bad?
    /// Using `super()` in a method of a `NamedTuple` class will raise an exception at runtime.
    ///
    /// ## Examples
    /// ```python
    /// from typing import NamedTuple
    ///
    /// class F(NamedTuple):
    ///     x: int
    ///
    ///     def method(self):
    ///         super()  # error: super() is not supported in methods of NamedTuple classes
    /// ```
    ///
    /// ## References
    /// - [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)
    pub(crate) static SUPER_CALL_IN_NAMED_TUPLE_METHOD = {
        summary: "detects `super()` calls in methods of `NamedTuple` classes",
        status: LintStatus::preview("0.0.1-alpha.30"),
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
    pub static UNDEFINED_REVEAL = {
        summary: "detects usages of `reveal_type` without importing it",
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for keyword arguments in calls that match positional-only parameters of the callable.
    ///
    /// ## Why is this bad?
    /// Providing a positional-only parameter as a keyword argument will raise `TypeError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// def f(x: int, /) -> int: ...
    ///
    /// f(x=1)  # Error raised here
    /// ```
    pub(crate) static POSITIONAL_ONLY_PARAMETER_AS_KWARG = {
        summary: "detects positional-only parameters passed as keyword arguments",
        status: LintStatus::stable("0.0.1-alpha.22"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
    pub static UNRESOLVED_REFERENCE = {
        summary: "detects references to names that are not defined",
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
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
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Detects variables declared as `global` in an inner scope that have no explicit
    /// bindings or declarations in the global scope.
    ///
    /// ## Why is this bad?
    /// Function bodies with `global` statements can run in any order (or not at all), which makes
    /// it hard for static analysis tools to infer the types of globals without
    /// explicit definitions or declarations.
    ///
    /// ## Example
    /// ```python
    /// def f():
    ///     global x  # unresolved global
    ///     x = 42
    ///
    /// def g():
    ///     print(x)  # unresolved reference
    /// ```
    ///
    /// Use instead:
    ///
    /// ```python
    /// x: int
    ///
    /// def f():
    ///     global x
    ///     x = 42
    ///
    /// def g():
    ///     print(x)
    /// ```
    ///
    /// Or:
    ///
    /// ```python
    /// x: int | None = None
    ///
    /// def f():
    ///     global x
    ///     x = 42
    ///
    /// def g():
    ///     print(x)
    /// ```
    pub(crate) static UNRESOLVED_GLOBAL = {
        summary: "detects `global` statements with no definition in the global scope",
        status: LintStatus::stable("0.0.1-alpha.15"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Detects missing required keys in `TypedDict` constructor calls.
    ///
    /// ## Why is this bad?
    /// `TypedDict` requires all non-optional keys to be provided during construction.
    /// Missing items can lead to a `KeyError` at runtime.
    ///
    /// ## Example
    /// ```python
    /// from typing import TypedDict
    ///
    /// class Person(TypedDict):
    ///     name: str
    ///     age: int
    ///
    /// alice: Person = {"name": "Alice"}  # missing required key 'age'
    ///
    /// alice["age"]  # KeyError
    /// ```
    pub(crate) static MISSING_TYPED_DICT_KEY = {
        summary: "detects missing required keys in `TypedDict` constructors",
        status: LintStatus::stable("0.0.1-alpha.20"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Detects method overrides that violate the [Liskov Substitution Principle] ("LSP").
    ///
    /// The LSP states that an instance of a subtype should be substitutable for an instance of its supertype.
    /// Applied to Python, this means:
    /// 1. All argument combinations a superclass method accepts
    ///    must also be accepted by an overriding subclass method.
    /// 2. The return type of an overriding subclass method must be a subtype
    ///    of the return type of the superclass method.
    ///
    /// ## Why is this bad?
    /// Violating the Liskov Substitution Principle will lead to many of ty's assumptions and
    /// inferences being incorrect, which will mean that it will fail to catch many possible
    /// type errors in your code.
    ///
    /// ## Example
    /// ```python
    /// class Super:
    ///     def method(self, x) -> int:
    ///         return 42
    ///
    /// class Sub(Super):
    ///     # Liskov violation: `str` is not a subtype of `int`,
    ///     # but the supertype method promises to return an `int`.
    ///     def method(self, x) -> str:  # error: [invalid-override]
    ///         return "foo"
    ///
    /// def accepts_super(s: Super) -> int:
    ///     return s.method(x=42)
    ///
    /// accepts_super(Sub())  # The result of this call is a string, but ty will infer
    ///                       # it to be an `int` due to the violation of the Liskov Substitution Principle.
    ///
    /// class Sub2(Super):
    ///     # Liskov violation: the superclass method can be called with a `x=`
    ///     # keyword argument, but the subclass method does not accept it.
    ///     def method(self, y) -> int:  # error: [invalid-override]
    ///        return 42
    ///
    /// accepts_super(Sub2())  # TypeError at runtime: method() got an unexpected keyword argument 'x'
    ///                        # ty cannot catch this error due to the violation of the Liskov Substitution Principle.
    /// ```
    ///
    /// ## Common issues
    ///
    /// ### Why does ty complain about my `__eq__` method?
    ///
    /// `__eq__` and `__ne__` methods in Python are generally expected to accept arbitrary
    /// objects as their second argument, for example:
    ///
    /// ```python
    /// class A:
    ///     x: int
    ///
    ///     def __eq__(self, other: object) -> bool:
    ///         # gracefully handle an object of an unexpected type
    ///         # without raising an exception
    ///         if not isinstance(other, A):
    ///             return False
    ///         return self.x == other.x
    /// ```
    ///
    /// If `A.__eq__` here were annotated as only accepting `A` instances for its second argument,
    /// it would imply that you wouldn't be able to use `==` between instances of `A` and
    /// instances of unrelated classes without an exception possibly being raised. While some
    /// classes in Python do indeed behave this way, the strongly held convention is that it should
    /// be avoided wherever possible. As part of this check, therefore, ty enforces that `__eq__`
    /// and `__ne__` methods accept `object` as their second argument.
    ///
    /// ### Why does ty disagree with Ruff about how to write my method?
    ///
    /// Ruff has several rules that will encourage you to rename a parameter, or change its type
    /// signature, if it thinks you're falling into a certain anti-pattern. For example, Ruff's
    /// [ARG002](https://docs.astral.sh/ruff/rules/unused-method-argument/) rule recommends that an
    /// unused parameter should either be removed or renamed to start with `_`. Applying either of
    /// these suggestions can cause ty to start reporting an `invalid-method-override` error if
    /// the function in question is a method on a subclass that overrides a method on a superclass,
    /// and the change would cause the subclass method to no longer accept all argument combinations
    /// that the superclass method accepts.
    ///
    /// This can usually be resolved by adding [`@typing.override`][override] to your method
    /// definition. Ruff knows that a method decorated with `@typing.override` is intended to
    /// override a method by the same name on a superclass, and avoids reporting rules like ARG002
    /// for such methods; it knows that the changes recommended by ARG002 would violate the Liskov
    /// Substitution Principle.
    ///
    /// Correct use of `@override` is enforced by ty's `invalid-explicit-override` rule.
    ///
    /// [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
    /// [override]: https://docs.python.org/3/library/typing.html#typing.override
    pub(crate) static INVALID_METHOD_OVERRIDE = {
        summary: "detects method definitions that violate the Liskov Substitution Principle",
        status: LintStatus::stable("0.0.1-alpha.20"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for dataclasses with invalid frozen inheritance:
    /// - A frozen dataclass cannot inherit from a non-frozen dataclass.
    /// - A non-frozen dataclass cannot inherit from a frozen dataclass.
    ///
    /// ## Why is this bad?
    /// Python raises a `TypeError` at runtime when either of these inheritance
    /// patterns occurs.
    ///
    /// ## Example
    ///
    /// ```python
    /// from dataclasses import dataclass
    ///
    /// @dataclass
    /// class Base:
    ///     x: int
    ///
    /// @dataclass(frozen=True)
    /// class Child(Base):  # Error raised here
    ///     y: int
    ///
    /// @dataclass(frozen=True)
    /// class FrozenBase:
    ///     x: int
    ///
    /// @dataclass
    /// class NonFrozenChild(FrozenBase):  # Error raised here
    ///     y: int
    /// ```
    pub(crate) static INVALID_FROZEN_DATACLASS_SUBCLASS = {
        summary: "detects dataclasses with invalid frozen/non-frozen subclassing",
        status: LintStatus::stable("0.0.1-alpha.35"),
        default_level: Level::Error,
    }
}

/// A collection of type check diagnostics.
#[derive(Default, Eq, PartialEq, get_size2::GetSize)]
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

    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.diagnostics.is_empty() && self.used_suppressions.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.diagnostics().iter()
    }

    fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.as_slice()
    }
}

impl std::fmt::Debug for TypeCheckDiagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.diagnostics().fmt(f)
    }
}

impl IntoIterator for TypeCheckDiagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_diagnostics().into_iter()
    }
}

impl<'a> IntoIterator for &'a TypeCheckDiagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Emit a diagnostic declaring that an index is out of bounds for a tuple.
pub(super) fn report_index_out_of_bounds(
    context: &InferContext,
    kind: &'static str,
    node: AnyNodeRef,
    tuple_ty: Type,
    length: impl std::fmt::Display,
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
pub(super) fn report_not_subscriptable(
    context: &InferContext,
    node: &ast::ExprSubscript,
    not_subscriptable_ty: Type,
    method: &str,
) {
    let Some(builder) = context.report_lint(&NOT_SUBSCRIPTABLE, node) else {
        return;
    };
    if method == "__delitem__" {
        builder.into_diagnostic(format_args!(
            "Cannot delete subscript on object of type `{}` with no `{method}` method",
            not_subscriptable_ty.display(context.db())
        ));
    } else {
        builder.into_diagnostic(format_args!(
            "Cannot subscript object of type `{}` with no `{method}` method",
            not_subscriptable_ty.display(context.db())
        ));
    }
}

pub(super) fn report_slice_step_size_zero(context: &InferContext, node: AnyNodeRef) {
    let Some(builder) = context.report_lint(&ZERO_STEPSIZE_IN_SLICE, node) else {
        return;
    };
    builder.into_diagnostic("Slice step size cannot be zero");
}

// We avoid emitting invalid assignment diagnostic for literal assignments to a `TypedDict`, as
// they can only occur if we already failed to validate the dict (and emitted some diagnostic).
pub(crate) fn is_invalid_typed_dict_literal(
    db: &dyn Db,
    target_ty: Type,
    source: AnyNodeRef<'_>,
) -> bool {
    target_ty
        .filter_union(db, Type::is_typed_dict)
        .as_typed_dict()
        .is_some()
        && matches!(source, AnyNodeRef::ExprDict(_))
}

fn report_invalid_assignment_with_message<'db, 'ctx: 'db, T: Ranged>(
    context: &'ctx InferContext,
    node: T,
    target_ty: Type<'db>,
    message: std::fmt::Arguments,
) -> Option<LintDiagnosticGuard<'db, 'ctx>> {
    let builder = context.report_lint(&INVALID_ASSIGNMENT, node)?;

    let mut diag = builder.into_diagnostic(message);

    match target_ty {
        Type::ClassLiteral(class) => {
            diag.info(format_args!(
                "Implicit shadowing of class `{}`, add an annotation to make it explicit if this is intentional",
                class.name(context.db()),
            ));
        }
        Type::FunctionLiteral(function) => {
            diag.info(format_args!(
                "Implicit shadowing of function `{}`, add an annotation to make it explicit if this is intentional",
                function.name(context.db()),
            ));
        }
        _ => {}
    }
    Some(diag)
}

pub(super) fn report_invalid_assignment<'db>(
    context: &InferContext<'db, '_>,
    target_node: AnyNodeRef,
    definition: Definition<'db>,
    target_ty: Type,
    value_ty: Type<'db>,
) {
    let definition_kind = definition.kind(context.db());
    let value_node = match definition_kind {
        DefinitionKind::Assignment(def) => Some(def.value(context.module())),
        DefinitionKind::AnnotatedAssignment(def) => def.value(context.module()),
        DefinitionKind::NamedExpression(def) => Some(&*def.node(context.module()).value),
        _ => None,
    };

    if let Some(value_node) = value_node
        && is_invalid_typed_dict_literal(context.db(), target_ty, value_node.into())
    {
        return;
    }

    let settings =
        DisplaySettings::from_possibly_ambiguous_types(context.db(), [target_ty, value_ty]);

    let diagnostic_range = if let Some(value_node) = value_node {
        // Expand the range to include parentheses around the value, if any. This allows
        // invalid-assignment diagnostics to be suppressed on the opening or closing parenthesis:
        // ```py
        // x: str = ( # ty: ignore <- here
        //     1 + 2 + 3
        // )  # ty: ignore <- or here
        // ```

        parentheses_iterator(value_node.into(), None, context.module().tokens())
            .last()
            .unwrap_or(value_node.range())
    } else {
        target_node.range()
    };

    let Some(mut diag) = report_invalid_assignment_with_message(
        context,
        diagnostic_range,
        target_ty,
        format_args!(
            "Object of type `{}` is not assignable to `{}`",
            value_ty.display_with(context.db(), settings.clone()),
            target_ty.display_with(context.db(), settings)
        ),
    ) else {
        return;
    };

    if value_node.is_some() {
        match definition_kind {
            DefinitionKind::AnnotatedAssignment(assignment) => {
                // For annotated assignments, just point to the annotation in the source code.
                diag.annotate(
                    context
                        .secondary(assignment.annotation(context.module()))
                        .message("Declared type"),
                );
            }
            _ => {
                // Otherwise, annotate the target with its declared type.
                diag.annotate(context.secondary(target_node).message(format_args!(
                    "Declared type `{}`",
                    target_ty.display(context.db()),
                )));
            }
        }

        diag.set_primary_message(format_args!(
            "Incompatible value of type `{}`",
            value_ty.display(context.db()),
        ));

        // Overwrite the concise message to avoid showing the value type twice
        let message = diag.primary_message().to_string();
        diag.set_concise_message(message);
    }
}

pub(super) fn report_invalid_attribute_assignment(
    context: &InferContext,
    node: AnyNodeRef,
    target_ty: Type,
    source_ty: Type,
    attribute_name: &'_ str,
) {
    // TODO: Ideally we would not emit diagnostics for `TypedDict` literal arguments
    // here (see `diagnostic::is_invalid_typed_dict_literal`). However, we may have
    // silenced diagnostics during attribute resolution, and rely on the assignability
    // diagnostic being emitted here.

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

pub(super) fn report_bad_dunder_set_call<'db>(
    context: &InferContext<'db, '_>,
    dunder_set_failure: &CallError<'db>,
    attribute: &str,
    object_type: Type<'db>,
    target: &ast::ExprAttribute,
) {
    let Some(builder) = context.report_lint(&INVALID_ASSIGNMENT, target) else {
        return;
    };
    let db = context.db();
    if let Some(property) = dunder_set_failure.as_attempt_to_set_property_with_no_setter() {
        let object_type = object_type.display(db);
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot assign to read-only property `{attribute}` on object of type `{object_type}`",
        ));
        if let Some(file_range) = property
            .getter(db)
            .and_then(|getter| getter.definition(db))
            .and_then(|definition| definition.focus_range(db))
        {
            diagnostic.annotate(Annotation::secondary(Span::from(file_range)).message(
                format_args!("Property `{object_type}.{attribute}` defined here with no setter"),
            ));
            diagnostic.set_primary_message(format_args!(
                "Attempted assignment to `{object_type}.{attribute}` here"
            ));
        }
    } else {
        // TODO: Here, it would be nice to emit an additional diagnostic
        // that explains why the call failed
        builder.into_diagnostic(format_args!(
            "Invalid assignment to data descriptor attribute \
            `{attribute}` on type `{}` with custom `__set__` method",
            object_type.display(db)
        ));
    }
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

    let settings =
        DisplaySettings::from_possibly_ambiguous_types(context.db(), [expected_ty, actual_ty]);
    let return_type_span = context.span(return_type_range);

    let mut diag = builder.into_diagnostic("Return type does not match returned value");
    diag.set_primary_message(format_args!(
        "expected `{expected_ty}`, found `{actual_ty}`",
        expected_ty = expected_ty.display_with(context.db(), settings.clone()),
        actual_ty = actual_ty.display_with(context.db(), settings.clone()),
    ));
    diag.annotate(
        Annotation::secondary(return_type_span).message(format_args!(
            "Expected `{expected_ty}` because of return type",
            expected_ty = expected_ty.display_with(context.db(), settings),
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
    enclosing_class_of_method: Option<ClassType>,
    no_return: bool,
) {
    let Some(builder) = context.report_lint(&INVALID_RETURN_TYPE, range) else {
        return;
    };
    let db = context.db();

    // If no return statement is defined in the function, then the function always returns `None`
    let mut diagnostic = if no_return {
        let mut diag = builder.into_diagnostic(format_args!(
            "Function always implicitly returns `None`, which is not assignable to return type `{}`",
            expected_ty.display(db),
        ));
        diag.info(
            "Consider changing the return annotation to `-> None` or adding a `return` statement",
        );
        diag
    } else {
        builder.into_diagnostic(format_args!(
            "Function can implicitly return `None`, which is not assignable to return type `{}`",
            expected_ty.display(db),
        ))
    };
    if !has_empty_body {
        return;
    }
    diagnostic.info(
        "Only functions in stub files, methods on protocol classes, \
            or methods with `@abstractmethod` are permitted to have empty bodies",
    );
    let Some(class) = enclosing_class_of_method else {
        return;
    };
    if class.iter_mro(db).contains(&ClassBase::Protocol) {
        diagnostic.info(format_args!(
            "Class `{}` has `typing.Protocol` in its MRO, but it is not a protocol class",
            class.name(db)
        ));

        let mut sub_diagnostic = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
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

pub(super) fn report_possibly_missing_attribute(
    context: &InferContext,
    target: &ast::ExprAttribute,
    attribute: &str,
    object_ty: Type,
) {
    let Some(builder) = context.report_lint(&POSSIBLY_MISSING_ATTRIBUTE, target) else {
        return;
    };
    let db = context.db();
    match object_ty {
        Type::ModuleLiteral(module) => builder.into_diagnostic(format_args!(
            "Member `{attribute}` may be missing on module `{}`",
            module.module(db).name(db),
        )),
        Type::ClassLiteral(class) => builder.into_diagnostic(format_args!(
            "Attribute `{attribute}` may be missing on class `{}`",
            class.name(db),
        )),
        Type::GenericAlias(alias) => builder.into_diagnostic(format_args!(
            "Attribute `{attribute}` may be missing on class `{}`",
            alias.display(db),
        )),
        _ => builder.into_diagnostic(format_args!(
            "Attribute `{attribute}` may be missing on object of type `{}`",
            object_ty.display(db),
        )),
    };
}

pub(super) fn report_invalid_exception_tuple_caught<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    node: &'ast ast::ExprTuple,
    node_type: Type<'db>,
    invalid_tuple_nodes: impl IntoIterator<Item = (&'ast ast::Expr, Type<'db>)>,
) {
    let Some(builder) = context.report_lint(&INVALID_EXCEPTION_CAUGHT, node) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic("Invalid tuple caught in an exception handler");
    diagnostic.set_concise_message(format_args!(
        "Cannot catch object of type `{}` in an exception handler",
        node_type.display(context.db())
    ));

    for (sub_node, ty) in invalid_tuple_nodes {
        let span = context.span(sub_node);
        diagnostic.annotate(Annotation::secondary(span.clone()).message(format_args!(
            "Invalid element of type `{}`",
            ty.display(context.db())
        )));
        if ty.is_notimplemented(context.db()) {
            diagnostic.annotate(
                Annotation::secondary(span).message("Did you mean `NotImplementedError`?"),
            );
        }
    }

    diagnostic.info(
        "Can only catch a subclass of `BaseException` or tuple of `BaseException` subclasses",
    );
}

pub(super) fn report_invalid_exception_caught(context: &InferContext, node: &ast::Expr, ty: Type) {
    let Some(builder) = context.report_lint(&INVALID_EXCEPTION_CAUGHT, node) else {
        return;
    };

    let mut diagnostic = if ty.is_notimplemented(context.db()) {
        let mut diag =
            builder.into_diagnostic("Cannot catch `NotImplemented` in an exception handler");
        diag.set_primary_message("Did you mean `NotImplementedError`?");
        diag
    } else {
        let mut diag = builder.into_diagnostic(format_args!(
            "Invalid {thing} caught in an exception handler",
            thing = if ty.tuple_instance_spec(context.db()).is_some() {
                "tuple"
            } else {
                "object"
            },
        ));
        diag.set_primary_message(format_args!(
            "Object has type `{}`",
            ty.display(context.db())
        ));
        diag
    };

    diagnostic.info(
        "Can only catch a subclass of `BaseException` or tuple of `BaseException` subclasses",
    );
}

pub(crate) fn report_invalid_exception_raised(
    context: &InferContext,
    raised_node: &ast::Expr,
    raise_type: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_RAISE, raised_node) else {
        return;
    };
    if raise_type.is_notimplemented(context.db()) {
        let mut diagnostic =
            builder.into_diagnostic(format_args!("Cannot raise `NotImplemented`",));
        diagnostic.set_primary_message("Did you mean `NotImplementedError`?");
        diagnostic.info("Can only raise an instance or subclass of `BaseException`");
    } else {
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot raise object of type `{}`",
            raise_type.display(context.db())
        ));
        diagnostic.set_primary_message("Not an instance or subclass of `BaseException`");
    }
}

pub(crate) fn report_invalid_exception_cause(context: &InferContext, node: &ast::Expr, ty: Type) {
    let Some(builder) = context.report_lint(&INVALID_RAISE, node) else {
        return;
    };
    let mut diagnostic = if ty.is_notimplemented(context.db()) {
        let mut diag = builder.into_diagnostic(format_args!(
            "Cannot use `NotImplemented` as an exception cause",
        ));
        diag.set_primary_message("Did you mean `NotImplementedError`?");
        diag
    } else {
        builder.into_diagnostic(format_args!(
            "Cannot use object of type `{}` as an exception cause",
            ty.display(context.db())
        ))
    };
    diagnostic.info(
        "An exception cause must be an instance of `BaseException`, \
        subclass of `BaseException`, or `None`",
    );
}

pub(crate) fn report_instance_layout_conflict(
    context: &InferContext,
    class: ClassLiteral,
    node: &ast::StmtClassDef,
    disjoint_bases: &IncompatibleBases,
) {
    debug_assert!(disjoint_bases.len() > 1);

    let db = context.db();

    let Some(builder) = context.report_lint(&INSTANCE_LAYOUT_CONFLICT, class.header_range(db))
    else {
        return;
    };

    let mut diagnostic = builder
        .into_diagnostic("Class will raise `TypeError` at runtime due to incompatible bases");

    diagnostic.set_primary_message(format_args!(
        "Bases {} cannot be combined in multiple inheritance",
        disjoint_bases.describe_problematic_class_bases(db)
    ));

    let mut subdiagnostic = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        "Two classes cannot coexist in a class's MRO if their instances \
        have incompatible memory layouts",
    );

    for (disjoint_base, disjoint_base_info) in disjoint_bases {
        let IncompatibleBaseInfo {
            node_index,
            originating_base,
        } = disjoint_base_info;

        let span = context.span(&node.bases()[*node_index]);
        let mut annotation = Annotation::secondary(span.clone());
        if disjoint_base.class == *originating_base {
            match disjoint_base.kind {
                DisjointBaseKind::DefinesSlots => {
                    annotation = annotation.message(format_args!(
                        "`{base}` instances have a distinct memory layout because `{base}` defines non-empty `__slots__`",
                        base = originating_base.name(db)
                    ));
                }
                DisjointBaseKind::DisjointBaseDecorator => {
                    annotation = annotation.message(format_args!(
                        "`{base}` instances have a distinct memory layout because of the way `{base}` \
                        is implemented in a C extension",
                        base = originating_base.name(db)
                    ));
                }
            }
            subdiagnostic.annotate(annotation);
        } else {
            annotation = annotation.message(format_args!(
                "`{base}` instances have a distinct memory layout \
                because `{base}` inherits from `{disjoint_base}`",
                base = originating_base.name(db),
                disjoint_base = disjoint_base.class.name(db)
            ));
            subdiagnostic.annotate(annotation);

            let mut additional_annotation = Annotation::secondary(span);

            additional_annotation = match disjoint_base.kind {
                DisjointBaseKind::DefinesSlots => additional_annotation.message(format_args!(
                    "`{disjoint_base}` instances have a distinct memory layout because `{disjoint_base}` \
                        defines non-empty `__slots__`",
                    disjoint_base = disjoint_base.class.name(db),
                )),

                DisjointBaseKind::DisjointBaseDecorator => {
                    additional_annotation.message(format_args!(
                        "`{disjoint_base}` instances have a distinct memory layout \
                        because of the way `{disjoint_base}` is implemented in a C extension",
                        disjoint_base = disjoint_base.class.name(db),
                    ))
                }
            };

            subdiagnostic.annotate(additional_annotation);
        }
    }

    diagnostic.sub(subdiagnostic);
}

/// Information regarding the conflicting disjoint bases a class is inferred to have in its MRO.
///
/// For each disjoint base, we record information about which element in the class's bases list
/// caused the disjoint base to be included in the class's MRO.
///
/// The inner data is an `IndexMap` to ensure that diagnostics regarding conflicting disjoint bases
/// are reported in a stable order.
#[derive(Debug, Default)]
pub(super) struct IncompatibleBases<'db>(FxIndexMap<DisjointBase<'db>, IncompatibleBaseInfo<'db>>);

impl<'db> IncompatibleBases<'db> {
    pub(super) fn insert(
        &mut self,
        base: DisjointBase<'db>,
        node_index: usize,
        class: ClassLiteral<'db>,
    ) {
        let info = IncompatibleBaseInfo {
            node_index,
            originating_base: class,
        };
        self.0.insert(base, info);
    }

    /// List the problematic class bases in a human-readable format.
    fn describe_problematic_class_bases(&self, db: &dyn Db) -> String {
        let bad_base_names = self.0.values().map(|info| info.originating_base.name(db));

        format_enumeration(bad_base_names)
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    /// Two disjoint bases are allowed to coexist in an MRO if one is a subclass of the other.
    /// This method therefore removes any entry in `self` that is a subclass of one or more
    /// other entries also contained in `self`.
    pub(super) fn remove_redundant_entries(&mut self, db: &'db dyn Db) {
        self.0 = self
            .0
            .iter()
            .filter(|(disjoint_base, _)| {
                self.0
                    .keys()
                    .filter(|other_base| other_base != disjoint_base)
                    .all(|other_base| {
                        !disjoint_base.class.is_subclass_of(
                            db,
                            None,
                            other_base.class.default_specialization(db),
                        )
                    })
            })
            .map(|(base, info)| (*base, *info))
            .collect();
    }
}

impl<'a, 'db> IntoIterator for &'a IncompatibleBases<'db> {
    type Item = (&'a DisjointBase<'db>, &'a IncompatibleBaseInfo<'db>);
    type IntoIter = indexmap::map::Iter<'a, DisjointBase<'db>, IncompatibleBaseInfo<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Information about which class base the "disjoint base" stems from
#[derive(Debug, Copy, Clone)]
pub(super) struct IncompatibleBaseInfo<'db> {
    /// The index of the problematic base in the [`ast::StmtClassDef`]'s bases list.
    node_index: usize,

    /// The base class in the [`ast::StmtClassDef`]'s bases list that caused
    /// the disjoint base to be included in the class's MRO.
    ///
    /// This won't necessarily be the same class as the `DisjointBase`'s class,
    /// as the `DisjointBase` may have found its way into the class's MRO by dint of it being a
    /// superclass of one of the classes in the class definition's bases list.
    originating_base: ClassLiteral<'db>,
}

pub(crate) fn report_invalid_arguments_to_annotated(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) else {
        return;
    };
    builder.into_diagnostic(
        "Special form `typing.Annotated` expected at least 2 arguments \
         (one type and at least one metadata element)",
    );
}

pub(crate) fn report_invalid_argument_number_to_special_form(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
    special_form: SpecialFormType,
    received_arguments: usize,
    expected_arguments: u8,
) {
    let noun = if expected_arguments == 1 {
        "type argument"
    } else {
        "type arguments"
    };
    if let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) {
        builder.into_diagnostic(format_args!(
            "Special form `{special_form}` expected exactly {expected_arguments} {noun}, \
            got {received_arguments}",
        ));
    }
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
        SubDiagnosticSeverity::Info,
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

pub(crate) fn report_bad_argument_to_protocol_interface(
    context: &InferContext,
    call: &ast::ExprCall,
    param_type: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call) else {
        return;
    };
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic("Invalid argument to `reveal_protocol_interface`");
    diagnostic
        .set_primary_message("Only protocol classes can be passed to `reveal_protocol_interface`");

    if let Some(class) = param_type.to_class_type(context.db()) {
        let mut class_def_diagnostic = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "`{}` is declared here, but it is not a protocol class:",
                class.name(db)
            ),
        );
        class_def_diagnostic.annotate(Annotation::primary(
            class.class_literal(db).0.header_span(db),
        ));
        diagnostic.sub(class_def_diagnostic);
    }

    diagnostic.info(
        "A class is only a protocol class if it directly inherits \
            from `typing.Protocol` or `typing_extensions.Protocol`",
    );
    // See TODO in `report_bad_argument_to_get_protocol_members` above
    diagnostic.info("See https://typing.python.org/en/latest/spec/protocol.html");
}

pub(crate) fn report_invalid_arguments_to_callable(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)",
    ));
}

pub(crate) fn add_type_expression_reference_link<'db, 'ctx>(
    mut diag: LintDiagnosticGuard<'db, 'ctx>,
) -> LintDiagnosticGuard<'db, 'ctx> {
    diag.info("See the following page for a reference on valid type expressions:");
    diag.info(
        "https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions",
    );
    diag
}

pub(crate) fn report_runtime_check_against_non_runtime_checkable_protocol(
    context: &InferContext,
    call: &ast::ExprCall,
    protocol: ProtocolClass,
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
        SubDiagnosticSeverity::Info,
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
    protocol: ProtocolClass,
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
        SubDiagnosticSeverity::Info,
        format_args!("Protocol classes cannot be instantiated"),
    );
    class_def_diagnostic.annotate(
        Annotation::primary(protocol.header_span(db))
            .message(format_args!("`{class_name}` declared as a protocol here")),
    );
    diagnostic.sub(class_def_diagnostic);
}

pub(crate) fn report_undeclared_protocol_member(
    context: &InferContext,
    definition: Definition,
    protocol_class: ProtocolClass,
    class_symbol_table: &PlaceTable,
) {
    /// We want to avoid suggesting an annotation for e.g. `x = None`,
    /// because the user almost certainly doesn't want to write `x: None = None`.
    /// We also want to avoid suggesting invalid syntax such as `x: <class 'int'> = int`.
    fn should_give_hint<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
        let class = match ty {
            Type::ProtocolInstance(ProtocolInstanceType {
                inner: Protocol::FromClass(_),
                ..
            }) => return true,
            Type::SubclassOf(subclass_of) => match subclass_of.subclass_of() {
                SubclassOfInner::Class(class) => class,
                SubclassOfInner::Dynamic(DynamicType::Any) => return true,
                SubclassOfInner::Dynamic(_) | SubclassOfInner::TypeVar(_) => return false,
            },
            Type::NominalInstance(instance) => instance.class(db),
            Type::Union(union) => {
                return union
                    .elements(db)
                    .iter()
                    .all(|elem| should_give_hint(db, *elem));
            }
            _ => return false,
        };

        !matches!(
            class.known(db),
            Some(KnownClass::NoneType | KnownClass::EllipsisType)
        )
    }

    let db = context.db();

    let Some(builder) = context.report_lint(
        &AMBIGUOUS_PROTOCOL_MEMBER,
        definition.full_range(db, context.module()),
    ) else {
        return;
    };

    let ScopedPlaceId::Symbol(symbol_id) = definition.place(db) else {
        return;
    };

    let symbol_name = class_symbol_table.symbol(symbol_id).name();
    let class_name = protocol_class.name(db);

    let mut diagnostic = builder
        .into_diagnostic("Cannot assign to undeclared variable in the body of a protocol class");

    if definition.kind(db).is_unannotated_assignment() {
        let binding_type = binding_type(db, definition);

        let suggestion = binding_type.promote_literals(db, TypeContext::default());

        if should_give_hint(db, suggestion) {
            diagnostic.set_primary_message(format_args!(
                "Consider adding an annotation, e.g. `{symbol_name}: {} = ...`",
                suggestion.display(db)
            ));
        } else {
            diagnostic.set_primary_message(format_args!(
                "Consider adding an annotation for `{symbol_name}`"
            ));
        }
    } else {
        diagnostic.set_primary_message(format_args!(
            "`{symbol_name}` is not declared as a protocol member"
        ));
    }

    let mut class_def_diagnostic = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        "Assigning to an undeclared variable in a protocol class \
    leads to an ambiguous interface",
    );
    class_def_diagnostic.annotate(
        Annotation::primary(protocol_class.header_span(db))
            .message(format_args!("`{class_name}` declared as a protocol here",)),
    );
    diagnostic.sub(class_def_diagnostic);

    diagnostic.info(format_args!(
        "No declarations found for `{symbol_name}` \
        in the body of `{class_name}` or any of its superclasses"
    ));
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
        SubDiagnosticSeverity::Info,
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

    if let Type::KnownInstance(KnownInstanceType::NewType(newtype)) = base_type {
        let Some(builder) = context.report_lint(&INVALID_BASE, base_node) else {
            return;
        };
        let mut diagnostic = builder.into_diagnostic("Cannot subclass an instance of NewType");
        diagnostic.info(format_args!(
            "Perhaps you were looking for: `{} = NewType('{}', {})`",
            class.name(context.db()),
            class.name(context.db()),
            newtype.name(context.db()),
        ));
        diagnostic.info(format_args!(
            "Definition of class `{}` will raise `TypeError` at runtime",
            class.name(context.db())
        ));
        return;
    }

    let tuple_of_types = Type::homogeneous_tuple(db, instance_of_type);

    let explain_mro_entries = |diagnostic: &mut LintDiagnosticGuard| {
        diagnostic.info(
            "An instance type is only a valid class base \
            if it has a valid `__mro_entries__` method",
        );
    };

    match base_type.try_call_dunder(
        db,
        "__mro_entries__",
        CallArguments::positional([tuple_of_types]),
        TypeContext::default(),
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
                        "Type `{}` may have an `__mro_entries__` attribute, but it may be missing",
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
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic("Unsupported class base");
    diagnostic.set_primary_message(format_args!("Has type `{}`", base_type.display(db)));
    diagnostic.set_concise_message(format_args!(
        "Unsupported class base with type `{}`",
        base_type.display(db)
    ));
    diagnostic.info(format_args!(
        "ty cannot resolve a consistent method resolution order (MRO) for class `{}` due to this base",
        class.name(db)
    ));
    diagnostic.info("Only class objects or `Any` are supported as class bases");
}

fn report_invalid_base<'ctx, 'db>(
    context: &'ctx InferContext<'db, '_>,
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

pub(crate) fn report_invalid_key_on_typed_dict<'db>(
    context: &InferContext<'db, '_>,
    typed_dict_node: AnyNodeRef,
    key_node: AnyNodeRef,
    typed_dict_ty: Type<'db>,
    full_object_ty: Option<Type<'db>>,
    key_ty: Type<'db>,
    items: &TypedDictSchema<'db>,
) {
    let db = context.db();
    if let Some(builder) = context.report_lint(&INVALID_KEY, key_node) {
        match key_ty {
            Type::StringLiteral(key) => {
                let key = key.value(db);
                let typed_dict_name = typed_dict_ty.display(db);

                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Unknown key \"{key}\" for TypedDict `{typed_dict_name}`",
                ));

                diagnostic.annotate(if let Some(full_object_ty) = full_object_ty {
                    context.secondary(typed_dict_node).message(format_args!(
                        "TypedDict `{typed_dict_name}` in {kind} type `{full_object_ty}`",
                        kind = if full_object_ty.is_union() {
                            "union"
                        } else {
                            "intersection"
                        },
                        full_object_ty = full_object_ty.display(db)
                    ))
                } else {
                    context
                        .secondary(typed_dict_node)
                        .message(format_args!("TypedDict `{typed_dict_name}`"))
                });

                let existing_keys = items.keys();
                if let Some(suggestion) = did_you_mean(existing_keys, key) {
                    if let AnyNodeRef::ExprStringLiteral(literal) = key_node {
                        let quoted_suggestion = format!(
                            "{quote}{suggestion}{quote}",
                            quote = literal.value.first_literal_flags().quote_str()
                        );
                        diagnostic
                            .set_primary_message(format_args!("Did you mean {quoted_suggestion}?"));
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            quoted_suggestion,
                            key_node.range(),
                        )));
                    } else {
                        diagnostic.set_primary_message(format_args!(
                            "Unknown key \"{key}\" - did you mean \"{suggestion}\"?",
                        ));
                    }
                    diagnostic.set_concise_message(format_args!(
                        "Unknown key \"{key}\" for TypedDict `{typed_dict_name}` - did you mean \"{suggestion}\"?",
                    ));
                } else {
                    diagnostic.set_primary_message(format_args!("Unknown key \"{key}\""));
                }
            }
            _ => {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "TypedDict `{}` can only be subscripted with a string literal key, \
                     got key of type `{}`",
                    typed_dict_ty.display(db),
                    key_ty.display(db),
                ));

                if let Some(full_object_ty) = full_object_ty {
                    diagnostic.info(format_args!(
                        "The full type of the subscripted object is `{}`",
                        full_object_ty.display(db)
                    ));
                }
            }
        }
    }
}

pub(super) fn report_namedtuple_field_without_default_after_field_with_default<'db>(
    context: &InferContext<'db, '_>,
    class: ClassLiteral<'db>,
    (field, field_def): (&str, Option<Definition<'db>>),
    (field_with_default, field_with_default_def): &(Name, Option<Definition<'db>>),
) {
    let db = context.db();
    let module = context.module();

    let diagnostic_range = field_def
        .map(|definition| definition.kind(db).full_range(module))
        .unwrap_or_else(|| class.header_range(db));

    let Some(builder) = context.report_lint(&INVALID_NAMED_TUPLE, diagnostic_range) else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(
        "NamedTuple field without default value cannot follow field(s) with default value(s)",
    );

    diagnostic.set_primary_message(format_args!(
        "Field `{field}` defined here without a default value",
    ));

    let Some(field_with_default_range) =
        field_with_default_def.map(|definition| definition.kind(db).full_range(module))
    else {
        return;
    };

    // If the end-of-scope definition in the class scope of the field-with-a-default-value
    // occurs after the range of the field-without-a-default-value,
    // avoid adding a subdiagnostic that points to the definition of the
    // field-with-a-default-value. It's confusing to talk about a field "before" the
    // field without the default value but then point to a definition that actually
    // occurs after the field without-a-default-value.
    if field_with_default_range.end() < diagnostic_range.start() {
        diagnostic.annotate(
            Annotation::secondary(context.span(field_with_default_range)).message(format_args!(
                "Earlier field `{field_with_default}` defined here with a default value",
            )),
        );
    } else {
        diagnostic.info(format_args!(
            "Earlier field `{field_with_default}` was defined with a default value",
        ));
    }
}

pub(super) fn report_named_tuple_field_with_leading_underscore<'db>(
    context: &InferContext<'db, '_>,
    class: ClassLiteral<'db>,
    field_name: &str,
    field_definition: Option<Definition<'db>>,
) {
    let db = context.db();
    let module = context.module();

    let diagnostic_range = field_definition
        .map(|definition| definition.kind(db).full_range(module))
        .unwrap_or_else(|| class.header_range(db));

    let Some(builder) = context.report_lint(&INVALID_NAMED_TUPLE, diagnostic_range) else {
        return;
    };
    let mut diagnostic =
        builder.into_diagnostic("NamedTuple field name cannot start with an underscore");

    if field_definition.is_some() {
        diagnostic.set_primary_message(
            "Class definition will raise `TypeError` at runtime due to this field",
        );
    } else {
        diagnostic.set_primary_message(format_args!(
            "Class definition will raise `TypeError` at runtime due to field `{field_name}`",
        ));
    }

    diagnostic.set_concise_message(format_args!(
        "NamedTuple field `{field_name}` cannot start with an underscore"
    ));
}

pub(crate) fn report_missing_typed_dict_key<'db>(
    context: &InferContext<'db, '_>,
    constructor_node: AnyNodeRef,
    typed_dict_ty: Type<'db>,
    missing_field: &str,
) {
    let db = context.db();
    if let Some(builder) = context.report_lint(&MISSING_TYPED_DICT_KEY, constructor_node) {
        let typed_dict_name = typed_dict_ty.display(db);
        builder.into_diagnostic(format_args!(
            "Missing required key '{missing_field}' in TypedDict `{typed_dict_name}` constructor",
        ));
    }
}

pub(crate) fn report_cannot_pop_required_field_on_typed_dict<'db>(
    context: &InferContext<'db, '_>,
    key_node: AnyNodeRef,
    typed_dict_ty: Type<'db>,
    field_name: &str,
) {
    let db = context.db();
    if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, key_node) {
        let typed_dict_name = typed_dict_ty.display(db);
        builder.into_diagnostic(format_args!(
            "Cannot pop required field '{field_name}' from TypedDict `{typed_dict_name}`",
        ));
    }
}

/// Enum representing the reason why a key cannot be deleted from a `TypedDict`.
#[derive(Copy, Clone)]
pub(crate) enum TypedDictDeleteErrorKind {
    /// The key exists but is required (not `NotRequired`)
    RequiredKey,
    /// The key does not exist in the `TypedDict`
    UnknownKey,
}

pub(crate) fn report_cannot_delete_typed_dict_key<'db>(
    context: &InferContext<'db, '_>,
    key_node: AnyNodeRef,
    typed_dict_ty: Type<'db>,
    field_name: &str,
    field: Option<&crate::types::typed_dict::TypedDictField<'db>>,
    error_kind: TypedDictDeleteErrorKind,
) {
    let db = context.db();
    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, key_node) else {
        return;
    };

    let typed_dict_name = typed_dict_ty.display(db);

    let mut diagnostic = match error_kind {
        TypedDictDeleteErrorKind::RequiredKey => builder.into_diagnostic(format_args!(
            "Cannot delete required key \"{field_name}\" from TypedDict `{typed_dict_name}`"
        )),
        TypedDictDeleteErrorKind::UnknownKey => builder.into_diagnostic(format_args!(
            "Cannot delete unknown key \"{field_name}\" from TypedDict `{typed_dict_name}`"
        )),
    };

    // Add sub-diagnostic pointing to where the field is defined (if available)
    if let Some(field) = field
        && let Some(declaration) = field.first_declaration()
    {
        let file = declaration.file(db);
        let module = parsed_module(db, file).load(db);

        let mut sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Field defined here");
        sub.annotate(
            Annotation::secondary(
                Span::from(file).with_range(declaration.full_range(db, &module).range()),
            )
            .message(format_args!(
                "`{field_name}` declared as required here; consider making it `NotRequired`"
            )),
        );
        diagnostic.sub(sub);
    }

    // Add hint about how to allow deletion
    if matches!(error_kind, TypedDictDeleteErrorKind::RequiredKey) {
        diagnostic.info(
            "Only keys marked as `NotRequired` (or in a TypedDict with `total=False`) can be deleted",
        );
    }
}

pub(crate) fn report_invalid_type_param_order<'db>(
    context: &InferContext<'db, '_>,
    class: ClassLiteral<'db>,
    node: &ast::StmtClassDef,
    typevar_with_default: TypeVarInstance<'db>,
    invalid_later_typevars: &[TypeVarInstance<'db>],
) {
    let db = context.db();

    let base_index = class
        .explicit_bases(db)
        .iter()
        .position(|base| {
            matches!(
                base,
                Type::KnownInstance(
                    KnownInstanceType::SubscriptedProtocol(_)
                        | KnownInstanceType::SubscriptedGeneric(_)
                )
            )
        })
        .expect(
            "It should not be possible for a class to have a legacy generic context \
            if it does not inherit from `Protocol[]` or `Generic[]`",
        );

    let base_node = &node.bases()[base_index];

    let primary_diagnostic_range = base_node
        .as_subscript_expr()
        .map(|subscript| &*subscript.slice)
        .unwrap_or(base_node)
        .range();

    let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, primary_diagnostic_range)
    else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(
        "Type parameters without defaults cannot follow type parameters with defaults",
    );

    diagnostic.set_concise_message(format_args!(
        "Type parameter `{}` without a default cannot follow earlier parameter `{}` with a default",
        invalid_later_typevars[0].name(db),
        typevar_with_default.name(db),
    ));

    if let [single_typevar] = invalid_later_typevars {
        diagnostic.set_primary_message(format_args!(
            "Type variable `{}` does not have a default",
            single_typevar.name(db),
        ));
    } else {
        let later_typevars =
            format_enumeration(invalid_later_typevars.iter().map(|tv| tv.name(db)));
        diagnostic.set_primary_message(format_args!(
            "Type variables {later_typevars} do not have defaults",
        ));
    }

    diagnostic.annotate(
        Annotation::primary(Span::from(context.file()).with_range(primary_diagnostic_range))
            .message(format_args!(
                "Earlier TypeVar `{}` does",
                typevar_with_default.name(db)
            )),
    );

    for tvar in [typevar_with_default, invalid_later_typevars[0]] {
        let Some(definition) = tvar.definition(db) else {
            continue;
        };
        let file = definition.file(db);
        diagnostic.annotate(
            Annotation::secondary(Span::from(
                definition.full_range(db, &parsed_module(db, file).load(db)),
            ))
            .message(format_args!("`{}` defined here", tvar.name(db))),
        );
    }
}

pub(crate) fn report_rebound_typevar<'db>(
    context: &InferContext<'db, '_>,
    typevar_name: &ast::name::Name,
    class: ClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    other_typevar: BoundTypeVarInstance<'db>,
) {
    let db = context.db();
    let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, class.header_range(db)) else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Generic class `{}` must not reference type variables bound in an enclosing scope",
        class_node.name,
    ));
    diagnostic.set_primary_message(format_args!(
        "`{typevar_name}` referenced in class definition here"
    ));
    let Some(other_definition) = other_typevar.binding_context(db).definition() else {
        return;
    };
    let span = match binding_type(db, other_definition) {
        Type::ClassLiteral(class) => Some(class.header_span(db)),
        Type::FunctionLiteral(function) => function.spans(db).map(|spans| spans.signature),
        _ => return,
    };
    if let Some(span) = span {
        diagnostic.annotate(Annotation::secondary(span).message(format_args!(
            "Type variable `{typevar_name}` is bound in this enclosing scope",
        )));
    }
}

// I tried refactoring this function to placate Clippy,
// but it did not improve readability! -- AW.
#[expect(clippy::too_many_arguments)]
pub(super) fn report_invalid_method_override<'db>(
    context: &InferContext<'db, '_>,
    member: &str,
    subclass: ClassType<'db>,
    subclass_definition: Definition<'db>,
    subclass_function: FunctionType<'db>,
    superclass: ClassType<'db>,
    superclass_type: Type<'db>,
    superclass_method_kind: MethodKind,
) {
    let db = context.db();

    let signature_span = |function: FunctionType<'db>| {
        function
            .literal(db)
            .last_definition(db)
            .spans(db)
            .map(|spans| spans.signature)
    };

    let subclass_definition_kind = subclass_definition.kind(db);
    let subclass_definition_signature_span = signature_span(subclass_function);

    // If the function was originally defined elsewhere and simply assigned
    // in the body of the class here, we cannot use the range associated with the `FunctionType`
    let diagnostic_range = if subclass_definition_kind.is_function_def() {
        subclass_definition_signature_span
            .as_ref()
            .and_then(Span::range)
            .unwrap_or_else(|| {
                subclass_function
                    .node(db, context.file(), context.module())
                    .range
            })
    } else {
        subclass_definition.full_range(db, context.module()).range()
    };

    let class_name = subclass.name(db);
    let superclass_name = superclass.name(db);

    let overridden_method = if class_name == superclass_name {
        format!(
            "{superclass}.{member}",
            superclass = superclass.qualified_name(db),
        )
    } else {
        format!("{superclass_name}.{member}")
    };

    let Some(builder) = context.report_lint(&INVALID_METHOD_OVERRIDE, diagnostic_range) else {
        return;
    };

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Invalid override of method `{member}`"));

    diagnostic.set_primary_message(format_args!(
        "Definition is incompatible with `{overridden_method}`"
    ));

    let class_member = |cls: ClassType<'db>| {
        cls.class_member(db, member, MemberLookupPolicy::default())
            .place
    };

    if let Place::Defined(Type::FunctionLiteral(subclass_function), _, _, _) =
        class_member(subclass)
        && let Place::Defined(Type::FunctionLiteral(superclass_function), _, _, _) =
            class_member(superclass)
        && let Ok(superclass_function_kind) =
            MethodDecorator::try_from_fn_type(db, superclass_function)
        && let Ok(subclass_function_kind) = MethodDecorator::try_from_fn_type(db, subclass_function)
        && superclass_function_kind != subclass_function_kind
    {
        diagnostic.info(format_args!(
            "`{class_name}.{member}` is {subclass_function_kind} \
            but `{overridden_method}` is {superclass_function_kind}",
            superclass_function_kind = superclass_function_kind.description(),
            subclass_function_kind = subclass_function_kind.description(),
        ));
    }

    diagnostic.info("This violates the Liskov Substitution Principle");

    if !subclass_definition_kind.is_function_def()
        && let Some(span) = subclass_definition_signature_span
    {
        diagnostic.annotate(
            Annotation::secondary(span)
                .message(format_args!("Signature of `{class_name}.{member}`")),
        );
    }

    let superclass_scope = superclass.class_literal(db).0.body_scope(db);

    match superclass_method_kind {
        MethodKind::NotSynthesized => {
            if let Some(superclass_symbol) = place_table(db, superclass_scope).symbol_id(member)
                && let Some(binding) = use_def_map(db, superclass_scope)
                    .end_of_scope_bindings(ScopedPlaceId::Symbol(superclass_symbol))
                    .next()
                && let Some(definition) = binding.binding.definition()
            {
                let definition_span = Span::from(
                    definition
                        .full_range(db, &parsed_module(db, superclass_scope.file(db)).load(db)),
                );

                let superclass_function_span = match superclass_type {
                    Type::FunctionLiteral(function) => signature_span(function),
                    Type::BoundMethod(method) => signature_span(method.function(db)),
                    _ => None,
                };

                let superclass_definition_kind = definition.kind(db);

                let secondary_span = if superclass_definition_kind.is_function_def()
                    && let Some(function_span) = superclass_function_span.clone()
                {
                    function_span
                } else {
                    definition_span
                };

                diagnostic.annotate(
                    Annotation::secondary(secondary_span.clone())
                        .message(format_args!("`{overridden_method}` defined here")),
                );

                if !superclass_definition_kind.is_function_def()
                    && let Some(function_span) = superclass_function_span
                    && function_span != secondary_span
                {
                    diagnostic.annotate(
                        Annotation::secondary(function_span)
                            .message(format_args!("Signature of `{overridden_method}`")),
                    );
                }
            }
        }
        MethodKind::Synthesized(class_kind) => {
            let make_sub =
                |message: fmt::Arguments| SubDiagnostic::new(SubDiagnosticSeverity::Info, message);

            let mut sub = match class_kind {
                CodeGeneratorKind::DataclassLike(_) => make_sub(format_args!(
                    "`{overridden_method}` is a generated method created because \
                        `{superclass_name}` is a dataclass"
                )),
                CodeGeneratorKind::NamedTuple => make_sub(format_args!(
                    "`{overridden_method}` is a generated method created because \
                        `{superclass_name}` inherits from `typing.NamedTuple`"
                )),
                CodeGeneratorKind::TypedDict => make_sub(format_args!(
                    "`{overridden_method}` is a generated method created because \
                        `{superclass_name}` is a `TypedDict`"
                )),
            };

            sub.annotate(
                Annotation::primary(superclass.header_span(db))
                    .message(format_args!("Definition of `{superclass_name}`")),
            );
            diagnostic.sub(sub);
        }
    }

    if superclass.is_object(db) && matches!(member, "__eq__" | "__ne__") {
        // Inspired by mypy's subdiagnostic at <https://github.com/python/mypy/blob/1b6ebb17b7fe64488a7b3c3b4b0187bb14fe331b/mypy/messages.py#L1307-L1318>
        let eq_subdiagnostics = [
            format_args!(
                "It is recommended for `{member}` to work with arbitrary objects, for example:",
            ),
            format_args!(""),
            format_args!("    def {member}(self, other: object) -> bool:",),
            format_args!("        if not isinstance(other, {class_name}):",),
            format_args!("            return False"),
            format_args!("        return <logic to compare two `{class_name}` instances>"),
            format_args!(""),
        ];

        for subdiag in eq_subdiagnostics {
            diagnostic.help(subdiag);
        }
    }
}

pub(super) fn report_overridden_final_method<'db>(
    context: &InferContext<'db, '_>,
    member: &str,
    subclass_definition: Definition<'db>,
    // N.B. the type of the *definition*, not the type on an instance of the subclass
    subclass_type: Type<'db>,
    superclass: ClassType<'db>,
    subclass: ClassType<'db>,
    superclass_method_defs: &[FunctionType<'db>],
) {
    let db = context.db();

    // Some hijinks so that we emit a diagnostic on the property getter rather than the property setter
    let property_getter_definition = if subclass_definition.kind(db).is_function_def()
        && let Type::PropertyInstance(property) = subclass_type
        && let Some(Type::FunctionLiteral(getter)) = property.getter(db)
    {
        let getter_definition = getter.definition(db);
        if getter_definition.scope(db) == subclass_definition.scope(db) {
            Some(getter_definition)
        } else {
            None
        }
    } else {
        None
    };

    let subclass_definition = property_getter_definition.unwrap_or(subclass_definition);

    let Some(builder) = context.report_lint(
        &OVERRIDE_OF_FINAL_METHOD,
        subclass_definition.focus_range(db, context.module()),
    ) else {
        return;
    };

    let superclass_name = if superclass.name(db) == subclass.name(db) {
        superclass.qualified_name(db).to_string()
    } else {
        superclass.name(db).to_string()
    };

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Cannot override `{superclass_name}.{member}`"));
    diagnostic.set_primary_message(format_args!(
        "Overrides a definition from superclass `{superclass_name}`"
    ));
    diagnostic.set_concise_message(format_args!(
        "Cannot override final member `{member}` from superclass `{superclass_name}`"
    ));

    let mut sub = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        format_args!(
            "`{superclass_name}.{member}` is decorated with `@final`, forbidding overrides"
        ),
    );

    let first_final_superclass_definition = superclass_method_defs
        .iter()
        .find(|function| function.has_known_decorator(db, FunctionDecorators::FINAL))
        .expect(
            "At least one function definition in the superclass should be decorated with `@final`",
        );

    let superclass_function_literal = if first_final_superclass_definition.file(db).is_stub(db) {
        first_final_superclass_definition.first_overload_or_implementation(db)
    } else {
        first_final_superclass_definition
            .literal(db)
            .last_definition(db)
    };

    sub.annotate(
        Annotation::secondary(Span::from(superclass_function_literal.focus_range(
            db,
            &parsed_module(db, first_final_superclass_definition.file(db)).load(db),
        )))
        .message(format_args!("`{superclass_name}.{member}` defined here")),
    );

    if let Some(decorator_span) =
        superclass_function_literal.find_known_decorator_span(db, KnownFunction::Final)
    {
        sub.annotate(Annotation::secondary(decorator_span));
    }

    diagnostic.sub(sub);

    // It's tempting to autofix properties as well,
    // but you'd want to delete the `@my_property.deleter` as well as the getter and the deleter,
    // and we don't model property deleters at all right now.
    if let Type::FunctionLiteral(function) = subclass_type {
        let class_node = subclass
            .class_literal(db)
            .0
            .body_scope(db)
            .node(db)
            .expect_class()
            .node(context.module());

        let (overloads, implementation) = function.overloads_and_implementation(db);
        let overload_count = overloads.len() + usize::from(implementation.is_some());
        let is_only = overload_count >= class_node.body.len();

        let overload_deletion = |overload: &OverloadLiteral<'db>| {
            let range = overload.node(db, context.file(), context.module()).range();
            if is_only {
                Edit::range_replacement("pass".to_string(), range)
            } else {
                Edit::range_deletion(range)
            }
        };

        let should_fix = overloads
            .iter()
            .copied()
            .chain(implementation)
            .all(|overload| {
                class_node
                    .body
                    .iter()
                    .filter_map(ast::Stmt::as_function_def_stmt)
                    .contains(overload.node(db, context.file(), context.module()))
            });

        match function.overloads_and_implementation(db) {
            ([first_overload, rest @ ..], None) => {
                diagnostic.help(format_args!("Remove all overloads for `{member}`"));
                diagnostic.set_optional_fix(should_fix.then(|| {
                    Fix::unsafe_edits(
                        overload_deletion(first_overload),
                        rest.iter().map(overload_deletion),
                    )
                }));
            }
            ([first_overload, rest @ ..], Some(implementation)) => {
                diagnostic.help(format_args!(
                    "Remove all overloads and the implementation for `{member}`"
                ));
                diagnostic.set_optional_fix(should_fix.then(|| {
                    Fix::unsafe_edits(
                        overload_deletion(first_overload),
                        rest.iter().chain([&implementation]).map(overload_deletion),
                    )
                }));
            }
            ([], Some(implementation)) => {
                diagnostic.help(format_args!("Remove the override of `{member}`"));
                diagnostic.set_optional_fix(
                    should_fix.then(|| Fix::unsafe_edit(overload_deletion(&implementation))),
                );
            }
            ([], None) => {
                // Should be impossible to get here: how would we even infer a function as a function
                // if it has 0 overloads and no implementation?
                unreachable!(
                    "A function should always have an implementation and/or >=1 overloads"
                );
            }
        }
    } else if let Type::PropertyInstance(property) = subclass_type
        && property.setter(db).is_some()
    {
        diagnostic.help(format_args!("Remove the getter and setter for `{member}`"));
    } else {
        diagnostic.help(format_args!("Remove the override of `{member}`"));
    }
}

pub(super) fn report_unsupported_comparison<'db>(
    context: &InferContext<'db, '_>,
    error: &UnsupportedComparisonError<'db>,
    range: TextRange,
    left: &ast::Expr,
    right: &ast::Expr,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
) {
    let db = context.db();

    let Some(diagnostic_builder) = context.report_lint(&UNSUPPORTED_OPERATOR, range) else {
        return;
    };

    let display_settings = DisplaySettings::from_possibly_ambiguous_types(
        db,
        [error.left_ty, error.right_ty, left_ty, right_ty],
    );

    let mut diagnostic =
        diagnostic_builder.into_diagnostic(format_args!("Unsupported `{}` operation", error.op));

    if left_ty == right_ty {
        diagnostic.set_primary_message(format_args!(
            "Both operands have type `{}`",
            left_ty.display_with(db, display_settings.clone())
        ));
        diagnostic.annotate(context.secondary(left));
        diagnostic.annotate(context.secondary(right));
        diagnostic.set_concise_message(format_args!(
            "Operator `{}` is not supported between two objects of type `{}`",
            error.op,
            left_ty.display_with(db, display_settings.clone())
        ));
    } else {
        for (ty, expr) in [(left_ty, left), (right_ty, right)] {
            diagnostic.annotate(context.secondary(expr).message(format_args!(
                "Has type `{}`",
                ty.display_with(db, display_settings.clone())
            )));
        }
        diagnostic.set_concise_message(format_args!(
            "Operator `{}` is not supported between objects of type `{}` and `{}`",
            error.op,
            left_ty.display_with(db, display_settings.clone()),
            right_ty.display_with(db, display_settings.clone())
        ));
    }

    // For non-atomic types like unions and tuples, we now provide context
    // on the underlying elements that caused the error.
    // If we're emitting a diagnostic for something like `(1, "foo") < (2, 3)`:
    //
    // - `left_ty` is `tuple[Literal[1], Literal["foo"]]`
    // - `right_ty` is `tuple[Literal[2], Literal[3]]
    // - `error.left_ty` is `Literal["foo"]`
    // - `error.right_ty` is `Literal[3]`
    if (error.left_ty, error.right_ty) != (left_ty, right_ty) {
        if let Some(TupleSpec::Fixed(lhs_spec)) = left_ty.tuple_instance_spec(db).as_deref()
            && let Some(TupleSpec::Fixed(rhs_spec)) = right_ty.tuple_instance_spec(db).as_deref()
            && lhs_spec.len() == rhs_spec.len()
            && let Some(position) = lhs_spec
                .all_elements()
                .iter()
                .zip(rhs_spec.all_elements())
                .position(|tup| tup == (&error.left_ty, &error.right_ty))
        {
            if error.left_ty == error.right_ty {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported between \
                    the tuple elements at index {} (both of type `{}`)",
                    error.op,
                    position + 1,
                    error.left_ty.display_with(db, display_settings),
                ));
            } else {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported between \
                    the tuple elements at index {} (of type `{}` and `{}`)",
                    error.op,
                    position + 1,
                    error.left_ty.display_with(db, display_settings.clone()),
                    error.right_ty.display_with(db, display_settings),
                ));
            }
        } else {
            if error.left_ty == error.right_ty {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported \
                    between two objects of type `{}`",
                    error.op,
                    error.left_ty.display_with(db, display_settings),
                ));
            } else {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported \
                    between objects of type `{}` and `{}`",
                    error.op,
                    error.left_ty.display_with(db, display_settings.clone()),
                    error.right_ty.display_with(db, display_settings)
                ));
            }
        }
    }
}

pub(super) fn report_unsupported_augmented_assignment<'db>(
    context: &InferContext<'db, '_>,
    stmt: &ast::StmtAugAssign,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
) {
    report_unsupported_binary_operation_impl(
        context,
        stmt.range(),
        &stmt.target,
        &stmt.value,
        left_ty,
        right_ty,
        OperatorDisplay {
            operator: stmt.op,
            is_augmented_assignment: true,
        },
    );
}

pub(super) fn report_unsupported_binary_operation<'db>(
    context: &InferContext<'db, '_>,
    binary_expression: &ast::ExprBinOp,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
    operator: ast::Operator,
) {
    let Some(mut diagnostic) = report_unsupported_binary_operation_impl(
        context,
        binary_expression.range(),
        &binary_expression.left,
        &binary_expression.right,
        left_ty,
        right_ty,
        OperatorDisplay {
            operator,
            is_augmented_assignment: false,
        },
    ) else {
        return;
    };
    let db = context.db();
    if operator == ast::Operator::BitOr
        && (left_ty.is_subtype_of(db, KnownClass::Type.to_instance(db))
            || right_ty.is_subtype_of(db, KnownClass::Type.to_instance(db)))
        && Program::get(db).python_version(db) < PythonVersion::PY310
    {
        diagnostic.info(
            "Note that `X | Y` PEP 604 union syntax is only available in Python 3.10 and later",
        );
        add_inferred_python_version_hint_to_diagnostic(db, &mut diagnostic, "resolving types");
    }
}

#[derive(Debug, Copy, Clone)]
struct OperatorDisplay {
    operator: ast::Operator,
    is_augmented_assignment: bool,
}

impl std::fmt::Display for OperatorDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_augmented_assignment {
            write!(f, "{}=", self.operator)
        } else {
            write!(f, "{}", self.operator)
        }
    }
}

fn report_unsupported_binary_operation_impl<'a>(
    context: &'a InferContext<'a, 'a>,
    range: TextRange,
    left: &ast::Expr,
    right: &ast::Expr,
    left_ty: Type<'a>,
    right_ty: Type<'a>,
    operator: OperatorDisplay,
) -> Option<LintDiagnosticGuard<'a, 'a>> {
    let db = context.db();
    let diagnostic_builder = context.report_lint(&UNSUPPORTED_OPERATOR, range)?;
    let display_settings = DisplaySettings::from_possibly_ambiguous_types(db, [left_ty, right_ty]);

    let mut diagnostic =
        diagnostic_builder.into_diagnostic(format_args!("Unsupported `{operator}` operation"));

    if left_ty == right_ty {
        diagnostic.set_primary_message(format_args!(
            "Both operands have type `{}`",
            left_ty.display_with(db, display_settings.clone())
        ));
        diagnostic.annotate(context.secondary(left));
        diagnostic.annotate(context.secondary(right));
        diagnostic.set_concise_message(format_args!(
            "Operator `{operator}` is not supported between two objects of type `{}`",
            left_ty.display_with(db, display_settings.clone())
        ));
    } else {
        for (ty, expr) in [(left_ty, left), (right_ty, right)] {
            diagnostic.annotate(context.secondary(expr).message(format_args!(
                "Has type `{}`",
                ty.display_with(db, display_settings.clone())
            )));
        }
        diagnostic.set_concise_message(format_args!(
            "Operator `{operator}` is not supported between objects of type `{}` and `{}`",
            left_ty.display_with(db, display_settings.clone()),
            right_ty.display_with(db, display_settings.clone())
        ));
    }

    Some(diagnostic)
}

pub(super) fn report_bad_frozen_dataclass_inheritance<'db>(
    context: &InferContext<'db, '_>,
    class: ClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    base_class: ClassLiteral<'db>,
    base_class_node: &ast::Expr,
    base_class_params: DataclassFlags,
) {
    let db = context.db();

    let Some(builder) =
        context.report_lint(&INVALID_FROZEN_DATACLASS_SUBCLASS, class.header_range(db))
    else {
        return;
    };

    let mut diagnostic = if base_class_params.is_frozen() {
        let mut diagnostic =
            builder.into_diagnostic("Non-frozen dataclass cannot inherit from frozen dataclass");
        diagnostic.set_concise_message(format_args!(
            "Non-frozen dataclass `{}` cannot inherit from frozen dataclass `{}`",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic.set_primary_message(format_args!(
            "Subclass `{}` is not frozen but base class `{}` is",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic
    } else {
        let mut diagnostic =
            builder.into_diagnostic("Frozen dataclass cannot inherit from non-frozen dataclass");
        diagnostic.set_concise_message(format_args!(
            "Frozen dataclass `{}` cannot inherit from non-frozen dataclass `{}`",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic.set_primary_message(format_args!(
            "Subclass `{}` is frozen but base class `{}` is not",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic
    };

    diagnostic.annotate(context.secondary(base_class_node));

    if let Some(position) = class.find_dataclass_decorator_position(db) {
        diagnostic.annotate(
            context
                .secondary(&class_node.decorator_list[position])
                .message(format_args!("`{}` dataclass parameters", class.name(db))),
        );
    }
    diagnostic.info("This causes the class creation to fail");

    if let Some(decorator_position) = base_class.find_dataclass_decorator_position(db) {
        let mut sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!("Base class definition"),
        );
        sub.annotate(
            Annotation::primary(base_class.header_span(db))
                .message(format_args!("`{}` definition", base_class.name(db))),
        );

        let base_class_file = base_class.file(db);
        let module = parsed_module(db, base_class_file).load(db);

        let decorator_range = base_class
            .body_scope(db)
            .node(db)
            .expect_class()
            .node(&module)
            .decorator_list[decorator_position]
            .range();

        sub.annotate(
            Annotation::secondary(Span::from(base_class_file).with_range(decorator_range)).message(
                format_args!("`{}` dataclass parameters", base_class.name(db)),
            ),
        );

        diagnostic.sub(sub);
    }
}

/// This function receives an unresolved `from foo import bar` import,
/// where `foo` can be resolved to a module but that module does not
/// have a `bar` member or submodule.
///
/// If the `foo` module originates from the standard library and `foo.bar`
/// *does* exist as a submodule in the standard library on *other* Python
/// versions, we add a hint to the diagnostic that the user may have
/// misconfigured their Python version.
///
/// The function returns `true` if a hint was added, `false` otherwise.
pub(super) fn hint_if_stdlib_submodule_exists_on_other_versions(
    db: &dyn Db,
    diagnostic: &mut Diagnostic,
    full_submodule_name: &ModuleName,
    parent_module: Module,
) -> bool {
    let Some(search_path) = parent_module.search_path(db) else {
        return false;
    };

    if !search_path.is_standard_library() {
        return false;
    }

    let program = Program::get(db);
    let typeshed_versions = program.search_paths(db).typeshed_versions();

    let Some(version_range) = typeshed_versions.exact(full_submodule_name) else {
        return false;
    };

    let python_version = program.python_version(db);
    if version_range.contains(python_version) {
        return false;
    }

    diagnostic.info(format_args!(
        "The stdlib module `{module_name}` only has a `{name}` \
            submodule on Python {version_range}",
        module_name = parent_module.name(db),
        name = full_submodule_name
            .components()
            .next_back()
            .expect("A `ModuleName` always has at least one component"),
        version_range = version_range.diagnostic_display(),
    ));

    add_inferred_python_version_hint_to_diagnostic(db, diagnostic, "resolving modules");

    true
}

/// This function receives an unresolved `foo.bar` attribute access,
/// where `foo` can be resolved to have a type but that type does not
/// have a `bar` attribute.
///
/// If the type of `foo` has a definition that originates in the
/// standard library and `foo.bar` *does* exist as an attribute on *other*
/// Python versions, we add a hint to the diagnostic that the user may have
/// misconfigured their Python version.
pub(super) fn hint_if_stdlib_attribute_exists_on_other_versions(
    db: &dyn Db,
    mut diagnostic: LintDiagnosticGuard,
    value_type: Type,
    attr: &str,
    action: &str,
) {
    // Currently we limit this analysis to attributes of stdlib modules,
    // as this covers the most important cases while not being too noisy
    // about basic typos or special types like `super(C, self)`
    let Type::ModuleLiteral(module_ty) = value_type else {
        return;
    };
    let module = module_ty.module(db);
    let Some(file) = module.file(db) else {
        return;
    };
    let Some(search_path) = module.search_path(db) else {
        return;
    };
    if !search_path.is_standard_library() {
        return;
    }

    // We populate place_table entries for stdlib items across all known versions and platforms,
    // so if this lookup succeeds then we know that this lookup *could* succeed with possible
    // configuration changes.
    let symbol_table = place_table(db, global_scope(db, file));
    let Some(symbol) = symbol_table.symbol_by_name(attr) else {
        return;
    };

    if !symbol.is_bound() {
        return;
    }

    diagnostic.info("The member may be available on other Python versions or platforms");

    // For now, we just mention the current version they're on, and hope that's enough of a nudge.
    // TODO: determine what version they need to be on
    // TODO: also mention the platform we're assuming
    // TODO: determine what platform they need to be on
    add_inferred_python_version_hint_to_diagnostic(db, &mut diagnostic, action);
}
