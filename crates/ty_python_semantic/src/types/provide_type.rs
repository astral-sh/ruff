//! Machine-readable printing of semantic types for provide-type.
//!
//! Type printing is deliberately separate from ordinary type display. Display is optimized for
//! people and may abbreviate or truncate a type. [`print_type`] instead emits the
//! endpoint's public, parseable representation or returns [`PrintTypeError`].
//!
//! Aliases are always preserved as references. Their values are never visited. This both preserves
//! the type identity selected by inference and makes named recursive aliases terminate naturally.
//!
//! # Canonical spellings
//!
//! Some semantic types have an internal representation that differs from their canonical source
//! spelling. In particular, ty represents the numeric-tower annotation `float` as the union of
//! `int` and an internal float-only type, and `complex` as the union of `int`, float-only, and
//! complex-only types. The printer recognizes those specific unions and prints the
//! semantics-preserving canonical `float` or `complex` spelling.
//!
//! # Provide-type normalizations
//!
//! Provide-type output favors public, parseable annotations over extensions that expose ty's
//! precise internal representation. It performs the following exhaustive set of normalizations
//! during printing, without constructing an intermediate [`Type`]:
//!
//! - ty's internal float-only and complex-only numeric-tower types are printed using the public
//!   `float` and `complex` spellings.
//! - Internal `Todo` types are printed as `Unknown`.
//! - Internal dynamic states for unknown generics, unspecialized type variables, invalid
//!   `Concatenate` expressions, and ambiguous overloads are widened to `typing.Any`.
//! - Compact enum complements such as `Color & ~Literal[Color.RED]` are expanded to the exact
//!   union of the remaining enum-member literals.
//! - Stable runtime type-form values are printed as exact `TypeOf` expressions. This includes
//!   special forms, named type variables and `NewType`s, unions, literals, generic aliases,
//!   callables, and sentinels.
//! - Runtime `Annotated` values are printed as their bare underlying type because ty does not
//!   retain their metadata.
//! - `TypeIs` and `TypeGuard` are printed as their public annotations without ty's internal
//!   narrowed-parameter binding.
//! - Parameter defaults without a single known literal value are printed as `...`. This includes
//!   defaults represented by the abstract `LiteralString` type.
//! - An unspecialized runtime PEP 695 alias object is printed as the canonical `TypeAliasType`
//!   class. A specialized alias object is printed as the canonical `GenericAlias` class, matching
//!   its runtime class.
//! - Direct synthesized-protocol conjuncts are omitted from intersections. Positive and negative
//!   conjuncts are treated the same. If omission leaves no independently printable conjunct, the
//!   type is unsupported.
//!
//! No other type is widened, resolved, or omitted. In particular, aliases remain references,
//! synthesized protocols outside an intersection remain unsupported, and synthesized `TypedDict`
//! types remain unsupported. These rules are recursive and position-independent: they apply
//! equally inside callables and invariant generic arguments. A failure at any nested position
//! makes the entire type unsupported.
//!
//! # Grammar
//!
//! The emitted language is described by this grammar. Names are fully-qualified lexical paths and
//! may contain dots in every name position, including after `def`. When multiple declarations
//! have the same lexical path, the ambiguous component receives a one-based source-order ordinal,
//! such as `module.C@1` or `module.Outer@2.C`.
//!
//! ```text
//! type          ::= union
//! union         ::= intersection (" | " intersection)*
//! intersection  ::= unary (" & " unary)*
//! unary         ::= "~" unary | primary
//! primary       ::= name
//!                 | free_typevar
//!                 | "None"
//!                 | "Module[" name "]"
//!                 | name "[" subscript_arguments "]"
//!                 | "(" type ")"
//!                 | callable
//! callable      ::= ["async "] "def " name binder? parameters " -> " type ": ..."
//!                 | binder? parameters " -> " type
//!                 | "Overloads[" callable (", " callable)+ "]"
//! binder        ::= "[" type_parameter (", " type_parameter)* "]"
//! type_parameter::= ["**"] identifier [": " type | ": (" arguments ")"]
//!                   [" = " type]
//! parameters    ::= "(" parameter_item (", " parameter_item)* ")" | "(...)"
//! parameter_item::= parameter | "/" | "*"
//! parameter     ::= ["*" | "**"] [identifier ": "] type [" = " default]
//! subscript_arguments ::= subscript_argument (", " subscript_argument)*
//! subscript_argument  ::= type | literal | "()" | "..." | "*" type
//! default       ::= literal | "None" | "..."
//! literal       ::= integer | "True" | "False" | string | bytes | name "." identifier
//! name          ::= name_component ("." name_component)*
//! name_component::= identifier ["@" integer]
//! free_typevar  ::= identifier ["." ("args" | "kwargs")] ["@" name]
//! ```
//!
//! Python syntax and precedence are used where possible. The experimental extensions are
//! module literals, callable expressions, overload groups, exact runtime-value `TypeOf`
//! expressions, scoped free type variables, the `Divergent` cycle marker, truthiness types,
//! intersections, and negation. Their precedence is `~`, then `&`, then `|`. The printer inserts
//! parentheses whenever a nested expression would otherwise change meaning.
//!
//! Named classes, aliases, functions, type variables, and `NewType`s are resolved by semantic
//! declaration identity. Lexical ordinals distinguish declarations that would otherwise have the
//! same name. They are stable only for a particular source snapshot: inserting, removing, or
//! reordering declarations can change them. Printing fails if an identity has no name. Except for
//! the provide-type omission described above, anonymous structural and inference-only types do not
//! acquire synthetic names; they are unsupported.

use std::fmt::{self, Write as _};

use ruff_db::parsed::parsed_module;
use ruff_python_ast::str::{Quote, TripleQuotes};
use ruff_python_literal::escape::{AsciiEscape, UnicodeEscape};
use rustc_hash::FxHashSet;
use thiserror::Error;
use ty_module_resolver::file_to_module;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeKind;
use ty_python_core::{semantic_index, use_def_map};

use super::callable::CallableTypeKind;
use super::function::FunctionType;
use super::generics::{GenericContext, Specialization};
use super::signatures::{CallableSignature, Parameter, Signature};
use super::tuple::TupleSpec;
use super::{
    BoundTypeVarInstance, ClassLiteral, ClassType, DynamicType, GenericAlias, KnownClass,
    KnownInstanceType, LiteralValueType, LiteralValueTypeKind, ParameterKind, SpecialFormType,
    SubclassOfInner, Type, TypeAliasType, TypeVarBoundOrConstraints, TypeVarKind, TypedDictType,
};
use crate::Db;

/// The reason why a semantic type has no supported provide-type spelling.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedTypeKind {
    /// A bound-method type used only internally.
    InternalBoundMethod,
    /// A callable type used only internally.
    InternalCallable,
    /// An anonymous callable whose kind is not regular.
    NonRegularAnonymousCallable,
    /// An anonymous synthesized protocol.
    SynthesizedProtocol,
    /// A runtime object from the typing system.
    RuntimeTypingObject,
    /// An instance of `property`.
    PropertyInstance,
    /// A bound `super` object.
    BoundSuper,
    /// An anonymous synthesized `TypedDict`.
    SynthesizedTypedDict,
    /// A generic specialization with a materialization policy.
    MaterializedGenericSpecialization,
    /// A fresh type variable used during inference.
    FreshInferenceTypeVariable,
    /// A synthetic type variable with no source binding.
    SyntheticTypeVariable,
    /// A callable with no signatures.
    EmptyCallable,
    /// A callable with the internal top parameter set.
    TopCallableParameters,
    /// A nominal type used only internally.
    InternalNominal,
}

impl fmt::Display for UnsupportedTypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InternalBoundMethod => "internal bound method",
            Self::InternalCallable => "internal callable type",
            Self::NonRegularAnonymousCallable => "non-regular anonymous callable",
            Self::SynthesizedProtocol => "synthesized protocol",
            Self::RuntimeTypingObject => "runtime typing object",
            Self::PropertyInstance => "property instance",
            Self::BoundSuper => "bound super type",
            Self::SynthesizedTypedDict => "synthesized TypedDict",
            Self::MaterializedGenericSpecialization => "materialized generic specialization",
            Self::FreshInferenceTypeVariable => "fresh inference type variable",
            Self::SyntheticTypeVariable => "synthetic type variable",
            Self::EmptyCallable => "empty callable",
            Self::TopCallableParameters => "top callable parameters",
            Self::InternalNominal => "internal nominal type",
        })
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PrintTypeError {
    /// The type has no supported provide-type spelling.
    ///
    /// For example, property descriptor objects have no supported provide-type spelling:
    ///
    /// ```python
    /// class C:
    ///     @property
    ///     def value(self) -> int: ...
    ///
    /// descriptor = C.value
    /// ```
    #[error("type `{kind}` cannot be printed")]
    UnsupportedType { kind: UnsupportedTypeKind },

    /// The type contains an anonymous structural cycle.
    ///
    /// A deferred `TypeOf` annotation can create a function type that contains itself:
    ///
    /// ```python
    /// from ty_extensions import TypeOf
    ///
    /// def recursive(value: "TypeOf[recursive]"): ...
    /// ```
    #[error("anonymous recursive type")]
    RecursiveType,

    /// A semantic declaration has no fully qualified lexical name.
    ///
    /// For example, a class created dynamically has a runtime name but no class declaration that
    /// can be named in the provide-type output:
    ///
    /// ```python
    /// C = type("C", (), {})
    /// value = C
    /// ```
    #[error("name `{name}` cannot be resolved")]
    UnresolvedName { name: String },
}

/// Prints the endpoint-specific public representation of `ty`.
pub fn print_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Result<String, PrintTypeError> {
    let mut printer = PrintType {
        db,
        output: String::new(),
        active: FxHashSet::default(),
        binders: Vec::new(),
    };
    printer.print(ty, Precedence::Callable)?;
    Ok(printer.output)
}

struct PrintType<'db> {
    db: &'db dyn Db,
    output: String,
    active: FxHashSet<Type<'db>>,
    binders: Vec<GenericContext<'db>>,
}

/// Ordered from weakest to strongest binding.
///
/// A child expression is parenthesized when its precedence is lower than the precedence required
/// by its parent. Keep the variants in grammar order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Precedence {
    Callable,
    Union,
    Intersection,
    Unary,
    Primary,
}

impl<'db> PrintType<'db> {
    fn print(
        &mut self,
        ty: Type<'db>,
        parent_precedence: Precedence,
    ) -> Result<(), PrintTypeError> {
        if !self.active.insert(ty) {
            return Err(PrintTypeError::RecursiveType);
        }

        let parenthesized = self.precedence(ty) < parent_precedence;
        if parenthesized {
            self.push('(');
        }
        self.print_inner(ty)?;

        if parenthesized {
            self.push(')');
        }

        self.active.remove(&ty);
        Ok(())
    }

    fn precedence(&self, ty: Type<'db>) -> Precedence {
        match ty {
            Type::KnownInstance(KnownInstanceType::Annotated(inner)) => {
                self.precedence(inner.inner(self.db))
            }
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::Callable(_) => {
                Precedence::Callable
            }
            Type::Union(_) => Precedence::Union,
            Type::EnumComplement(complement) => {
                self.precedence(complement.remaining_literal_union(self.db))
            }
            Type::Intersection(_) => Precedence::Intersection,
            _ => Precedence::Primary,
        }
    }

    fn print_inner(&mut self, ty: Type<'db>) -> Result<(), PrintTypeError> {
        match ty {
            Type::Dynamic(dynamic) => self.print_dynamic(dynamic),
            Type::Divergent(_) => self.push_str("Divergent"),
            Type::Never => {
                self.print_intrinsic("typing", "Never");
            }
            Type::FunctionLiteral(function) => {
                self.print_named_callable(function, function.signature(self.db))?;
            }
            Type::BoundMethod(method) => {
                self.print_named_callable(
                    method.function(self.db),
                    &method.bound_signatures(self.db),
                )?;
            }
            Type::KnownBoundMethod(_) => {
                return Self::unsupported(UnsupportedTypeKind::InternalBoundMethod);
            }
            Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_) => {
                return Self::unsupported(UnsupportedTypeKind::InternalCallable);
            }
            Type::Callable(callable) => {
                if callable.kind(self.db) != CallableTypeKind::Regular {
                    return Self::unsupported(UnsupportedTypeKind::NonRegularAnonymousCallable);
                }
                self.print_callable(callable.signatures(self.db), None, false)?;
            }
            Type::ModuleLiteral(module) => {
                self.push_str("Module[");
                self.push_str(module.module(self.db).name(self.db));
                self.push(']');
            }
            Type::ClassLiteral(class) => {
                self.print_intrinsic("ty_extensions", "TypeOf");
                self.push('[');
                self.print_class_literal(class)?;
                self.push(']');
            }
            Type::GenericAlias(alias) => {
                self.print_intrinsic("ty_extensions", "TypeOf");
                self.push('[');
                self.print_generic_alias(alias)?;
                self.push(']');
            }
            Type::SubclassOf(subclass) => {
                self.print_intrinsic("builtins", "type");
                self.push('[');
                match subclass.subclass_of() {
                    SubclassOfInner::Class(class) => self.print_class_type(class)?,
                    SubclassOfInner::Dynamic(dynamic) => self.print_dynamic(dynamic),
                    SubclassOfInner::TypeVar(typevar) => self.print_bound_typevar(typevar)?,
                }
                self.push(']');
            }
            Type::NominalInstance(instance) => {
                let class = instance.class(self.db);
                if class.known(self.db) == Some(KnownClass::NoneType) {
                    self.push_str("None");
                } else {
                    self.print_class_type(class)?;
                }
            }
            Type::ProtocolInstance(protocol) => {
                let Some(instance) = protocol.to_nominal_instance() else {
                    return Self::unsupported(UnsupportedTypeKind::SynthesizedProtocol);
                };
                self.print_class_type(instance.class(self.db))?;
            }
            Type::SpecialForm(special_form) => self.print_special_form(special_form),
            Type::KnownInstance(instance) => self.print_known_instance(instance)?,
            Type::PropertyInstance(_) => {
                return Self::unsupported(UnsupportedTypeKind::PropertyInstance);
            }
            Type::Union(union) => {
                if let Some(known) = union.known(self.db) {
                    self.print_intrinsic("builtins", known.name());
                } else {
                    let mut first = true;
                    for element in union.elements(self.db) {
                        self.write_separator(&mut first, " | ");
                        self.print(*element, Precedence::Union)?;
                    }
                }
            }
            Type::Intersection(intersection) => {
                let omit = |ty: &Type<'db>| Self::should_omit_intersection_conjunct(*ty);
                let mut first = true;
                for element in intersection.positive(self.db).iter().filter(|ty| !omit(ty)) {
                    self.write_separator(&mut first, " & ");
                    self.print(*element, Precedence::Intersection)?;
                }
                for element in intersection.negative(self.db).iter().filter(|ty| !omit(ty)) {
                    self.write_separator(&mut first, " & ");
                    self.push('~');
                    self.print(*element, Precedence::Unary)?;
                }
                if first {
                    return Self::unsupported(UnsupportedTypeKind::SynthesizedProtocol);
                }
            }
            Type::EnumComplement(complement) => self.print(
                complement.remaining_literal_union(self.db),
                Precedence::Callable,
            )?,
            Type::AlwaysTruthy => {
                self.print_intrinsic("ty_extensions", "AlwaysTruthy");
            }
            Type::AlwaysFalsy => {
                self.print_intrinsic("ty_extensions", "AlwaysFalsy");
            }
            Type::LiteralValue(literal) => self.print_literal_type(literal)?,
            Type::TypeVar(typevar) => self.print_bound_typevar(typevar)?,
            Type::BoundSuper(_) => return Self::unsupported(UnsupportedTypeKind::BoundSuper),
            Type::TypeIs(type_is) => self.print_parameterized_special_form(
                SpecialFormType::TypeIs,
                type_is.type_argument(self.db),
            )?,
            Type::TypeGuard(type_guard) => self.print_parameterized_special_form(
                SpecialFormType::TypeGuard,
                type_guard.return_type(self.db),
            )?,
            Type::TypeForm(type_form) => self.print_parameterized_special_form(
                SpecialFormType::TypeForm,
                type_form.type_argument(self.db),
            )?,
            Type::TypedDict(TypedDictType::Class(class)) => {
                self.print_class_type(class)?;
            }
            Type::TypedDict(TypedDictType::Synthesized(_)) => {
                return Self::unsupported(UnsupportedTypeKind::SynthesizedTypedDict);
            }
            Type::TypeAlias(alias) => self.print_alias(alias)?,
            Type::NewTypeInstance(newtype) => {
                let name =
                    self.definition_name(newtype.definition(self.db), newtype.name(self.db))?;
                self.push_str(&name);
            }
        }

        Ok(())
    }

    fn print_dynamic(&mut self, dynamic: DynamicType<'db>) {
        match dynamic {
            DynamicType::Any => self.print_intrinsic("typing", "Any"),
            DynamicType::Todo(_)
            | DynamicType::TodoUnpack
            | DynamicType::TodoStarredExpression
            | DynamicType::TodoTypeVarTuple
            | DynamicType::Unknown => self.push_str("Unknown"),
            DynamicType::UnknownGeneric(_)
            | DynamicType::UnspecializedTypeVar
            | DynamicType::InvalidConcatenateUnknown
            | DynamicType::AmbiguousOverload => self.print_intrinsic("typing", "Any"),
        }
    }

    fn print_special_form(&mut self, special_form: SpecialFormType) {
        match special_form {
            SpecialFormType::LegacyStdlibAlias(_)
            | SpecialFormType::TypeQualifier(_)
            | SpecialFormType::Tuple
            | SpecialFormType::Type
            | SpecialFormType::TypeForm
            | SpecialFormType::TypingCallable
            | SpecialFormType::CollectionsAbcCallable
            | SpecialFormType::Any
            | SpecialFormType::Annotated
            | SpecialFormType::Literal
            | SpecialFormType::LiteralString
            | SpecialFormType::Optional
            | SpecialFormType::Union
            | SpecialFormType::NoReturn
            | SpecialFormType::Never
            | SpecialFormType::Unknown
            | SpecialFormType::Divergent
            | SpecialFormType::AlwaysTruthy
            | SpecialFormType::AlwaysFalsy
            | SpecialFormType::Not
            | SpecialFormType::Intersection
            | SpecialFormType::TypeOf
            | SpecialFormType::CallableTypeOf
            | SpecialFormType::RegularCallableTypeOf
            | SpecialFormType::Top
            | SpecialFormType::Bottom
            | SpecialFormType::TypingSelf
            | SpecialFormType::Concatenate
            | SpecialFormType::Unpack
            | SpecialFormType::TypeAlias
            | SpecialFormType::TypeGuard
            | SpecialFormType::TypedDict
            | SpecialFormType::TypeIs
            | SpecialFormType::Protocol
            | SpecialFormType::Generic
            | SpecialFormType::NamedTuple => {
                self.print_intrinsic("ty_extensions", "TypeOf");
                self.push('[');
                let _ = write!(self.output, "{special_form}");
                self.push(']');
            }
        }
    }

    fn print_known_instance(
        &mut self,
        instance: KnownInstanceType<'db>,
    ) -> Result<(), PrintTypeError> {
        match instance {
            KnownInstanceType::TypeVar(typevar) => {
                let Some(definition) = typevar.definition(self.db) else {
                    return Err(PrintTypeError::UnresolvedName {
                        name: typevar.name(self.db).to_string(),
                    });
                };
                let name = self.definition_name(definition, typevar.name(self.db).as_str())?;
                self.print_named_type_of(&name);
            }
            KnownInstanceType::TypeAliasType(_) => {
                let name = self.known_class(instance.class(self.db))?;
                self.push_str(&name);
            }
            instance @ (KnownInstanceType::UnionType(_)
            | KnownInstanceType::Literal(_)
            | KnownInstanceType::TypeGenericAlias(_)
            | KnownInstanceType::Callable(_)
            | KnownInstanceType::LiteralStringAlias(_)
            | KnownInstanceType::NewType(_)) => {
                let Some(ty) = instance.type_form_argument(self.db) else {
                    return Self::unsupported(UnsupportedTypeKind::RuntimeTypingObject);
                };
                self.print_parameterized_special_form(SpecialFormType::TypeOf, ty)?;
            }
            KnownInstanceType::Annotated(ty) => {
                self.print(ty.inner(self.db), Precedence::Callable)?;
            }
            KnownInstanceType::Sentinel(sentinel) => {
                let name = self.definition_name(
                    sentinel.definition(self.db),
                    sentinel.name(self.db).as_str(),
                )?;
                self.print_named_type_of(&name);
            }
            KnownInstanceType::SubscriptedProtocol(_)
            | KnownInstanceType::SubscriptedGeneric(_)
            | KnownInstanceType::Deprecated(_)
            | KnownInstanceType::Field(_)
            | KnownInstanceType::ConstraintSet(_)
            | KnownInstanceType::GenericContext(_)
            | KnownInstanceType::Specialization(_)
            | KnownInstanceType::NamedTupleSpec(_)
            | KnownInstanceType::FunctoolsPartial(_) => {
                return Self::unsupported(UnsupportedTypeKind::RuntimeTypingObject);
            }
        }
        Ok(())
    }

    fn print_parameterized_special_form(
        &mut self,
        special_form: SpecialFormType,
        ty: Type<'db>,
    ) -> Result<(), PrintTypeError> {
        let _ = write!(self.output, "{special_form}");
        self.push('[');
        self.print(ty, Precedence::Callable)?;
        self.push(']');
        Ok(())
    }

    fn print_named_type_of(&mut self, name: &str) {
        self.print_intrinsic("ty_extensions", "TypeOf");
        self.push('[');
        self.push_str(name);
        self.push(']');
    }

    fn print_class_literal(&mut self, class: ClassLiteral<'db>) -> Result<String, PrintTypeError> {
        let name = if let Some(known) = class.known(self.db) {
            self.known_class(known)?
        } else {
            let Some(definition) = class.definition(self.db) else {
                return Err(PrintTypeError::UnresolvedName {
                    name: class.name(self.db).to_string(),
                });
            };
            self.definition_name(definition, class.name(self.db).as_str())?
        };
        self.push_str(&name);
        Ok(name)
    }

    fn print_class_type(&mut self, class: ClassType<'db>) -> Result<(), PrintTypeError> {
        match class {
            ClassType::NonGeneric(class) => {
                self.print_class_literal(class)?;
                Ok(())
            }
            ClassType::Generic(alias) => self.print_generic_alias(alias),
        }
    }

    fn should_omit_intersection_conjunct(ty: Type<'db>) -> bool {
        matches!(
            ty,
            Type::ProtocolInstance(protocol) if protocol.to_nominal_instance().is_none()
        )
    }

    fn print_generic_alias(&mut self, alias: GenericAlias<'db>) -> Result<(), PrintTypeError> {
        let origin = alias.origin(self.db);
        let name = self.print_class_literal(origin.into())?;
        self.print_specialization(&name, alias.specialization(self.db), origin.known(self.db))
    }

    fn print_specialization(
        &mut self,
        name: &str,
        specialization: Specialization<'db>,
        known: Option<KnownClass>,
    ) -> Result<(), PrintTypeError> {
        if specialization.materialization_kind(self.db).is_some() {
            return Self::unsupported(UnsupportedTypeKind::MaterializedGenericSpecialization);
        }

        if known == Some(KnownClass::Tuple)
            && let Some(tuple) = specialization.tuple(self.db)
        {
            return self.print_tuple(name, tuple);
        }

        self.push('[');
        for (index, ty) in specialization.types(self.db).iter().enumerate() {
            if index > 0 {
                self.push_str(", ");
            }
            self.print(*ty, Precedence::Callable)?;
        }
        self.push(']');
        Ok(())
    }

    fn print_tuple(&mut self, name: &str, tuple: &TupleSpec<'db>) -> Result<(), PrintTypeError> {
        self.push('[');
        let mut first = true;
        match tuple {
            TupleSpec::Fixed(fixed) => {
                for element in fixed.iter_all_elements() {
                    self.write_separator(&mut first, ", ");
                    self.print(element, Precedence::Callable)?;
                }
                if first {
                    self.push_str("()");
                }
            }
            TupleSpec::Variable(variable) => {
                for element in variable.iter_prefix_elements() {
                    self.write_separator(&mut first, ", ");
                    self.print(element, Precedence::Callable)?;
                }
                if first && variable.suffix_elements().is_empty() {
                    self.print(variable.variable(), Precedence::Callable)?;
                    self.push_str(", ...");
                } else {
                    self.write_separator(&mut first, ", ");
                    self.push('*');
                    self.push_str(name);
                    self.push('[');
                    self.print(variable.variable(), Precedence::Callable)?;
                    self.push_str(", ...]");
                    for element in variable.iter_suffix_elements() {
                        self.write_separator(&mut first, ", ");
                        self.print(element, Precedence::Callable)?;
                    }
                }
            }
        }
        self.push(']');
        Ok(())
    }

    fn print_alias(&mut self, alias: TypeAliasType<'db>) -> Result<(), PrintTypeError> {
        let name = self.definition_name(alias.definition(self.db), alias.name(self.db))?;
        self.push_str(&name);
        let Some(specialization) = alias.specialization(self.db) else {
            return Ok(());
        };
        self.print_specialization(&name, specialization, None)
    }

    fn print_bound_typevar(
        &mut self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<(), PrintTypeError> {
        let identity = typevar.identity(self.db);
        let attr = typevar.paramspec_attr(self.db);

        if typevar.kind(self.db) == TypeVarKind::TypingSelf {
            self.print_intrinsic("typing", "Self");
        } else if self
            .binders
            .iter()
            .rev()
            .any(|context| context.contains(self.db, identity))
        {
            self.push_str(typevar.name(self.db));
            if let Some(attr) = attr {
                let _ = write!(self.output, ".{attr}");
            }
        } else {
            if typevar.freshness(self.db).value() != 0 {
                return Self::unsupported(UnsupportedTypeKind::FreshInferenceTypeVariable);
            }

            let Some(binding_definition) = typevar.binding_context(self.db).definition() else {
                return Self::unsupported(UnsupportedTypeKind::SyntheticTypeVariable);
            };
            let name = attr.map_or_else(
                || typevar.name(self.db).to_string(),
                |attr| format!("{}.{attr}", typevar.name(self.db)),
            );
            let qualified_name = binding_definition
                .name(self.db)
                .and_then(|binding_name| {
                    self.qualified_definition_name(binding_definition, None, &binding_name)
                })
                .map(|binding_name| format!("{name}@{binding_name}"));
            let qualified_name = qualified_name.ok_or(PrintTypeError::UnresolvedName { name })?;
            self.push_str(&qualified_name);
        }
        Ok(())
    }

    fn print_named_callable(
        &mut self,
        function: FunctionType<'db>,
        signatures: &CallableSignature<'db>,
    ) -> Result<(), PrintTypeError> {
        let definition = function.definition(self.db);
        let name = self
            .qualified_definition_name(definition, Some(function), function.name(self.db))
            .ok_or_else(|| PrintTypeError::UnresolvedName {
                name: function.name(self.db).to_string(),
            })?;
        let is_async = self.function_is_async(definition);
        self.print_callable(signatures, Some(&name), is_async)
    }

    fn print_callable(
        &mut self,
        signatures: &CallableSignature<'db>,
        name: Option<&str>,
        is_async: bool,
    ) -> Result<(), PrintTypeError> {
        if signatures.overloads.is_empty() {
            return Self::unsupported(UnsupportedTypeKind::EmptyCallable);
        }

        let overloaded = signatures.overloads.len() > 1;
        if overloaded {
            self.push_str("Overloads[");
        }
        for (index, signature) in signatures.overloads.iter().enumerate() {
            if index > 0 {
                self.push_str(", ");
            }
            let signature_is_async = name.is_some()
                && signature
                    .definition
                    .map_or(is_async, |definition| self.function_is_async(definition));
            self.print_signature(signature, name, signature_is_async)?;
        }
        if overloaded {
            self.push(']');
        }
        Ok(())
    }

    fn print_signature(
        &mut self,
        signature: &Signature<'db>,
        name: Option<&str>,
        is_async: bool,
    ) -> Result<(), PrintTypeError> {
        if let Some(context) = signature.generic_context {
            self.binders.push(context);
        }

        if is_async {
            self.push_str("async ");
        }
        if let Some(name) = name {
            self.push_str("def ");
            self.push_str(name);
        }
        self.print_generic_context(signature.generic_context)?;
        self.print_parameters(signature.parameters())?;
        self.push_str(" -> ");
        let return_ty = if is_async {
            self.declared_async_return_type(signature.return_ty)
        } else {
            signature.return_ty
        };
        self.print(return_ty, Precedence::Callable)?;
        if name.is_some() {
            self.push_str(": ...");
        }

        if signature.generic_context.is_some() {
            self.binders.pop();
        }
        Ok(())
    }

    fn declared_async_return_type(&self, return_ty: Type<'db>) -> Type<'db> {
        let Type::NominalInstance(instance) = return_ty else {
            return return_ty;
        };
        let ClassType::Generic(alias) = instance.class(self.db) else {
            return return_ty;
        };
        if alias.origin(self.db).known(self.db) != Some(KnownClass::CoroutineType) {
            return return_ty;
        }

        alias
            .specialization(self.db)
            .types(self.db)
            .get(2)
            .copied()
            .unwrap_or(return_ty)
    }

    fn print_generic_context(
        &mut self,
        generic_context: Option<GenericContext<'db>>,
    ) -> Result<(), PrintTypeError> {
        let Some(generic_context) = generic_context else {
            return Ok(());
        };

        let mut first = true;
        for bound_typevar in generic_context.variables(self.db) {
            let typevar = bound_typevar.typevar(self.db);
            if typevar.is_self(self.db) {
                continue;
            }

            if first {
                self.push('[');
            } else {
                self.push_str(", ");
            }
            first = false;
            if typevar.is_paramspec(self.db) {
                self.push_str("**");
            }
            self.push_str(typevar.name(self.db));
            match typevar.bound_or_constraints(self.db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    self.push_str(": ");
                    self.print(bound, Precedence::Callable)?;
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    self.push_str(": (");
                    for (index, constraint) in constraints.elements(self.db).iter().enumerate() {
                        if index > 0 {
                            self.push_str(", ");
                        }
                        self.print(*constraint, Precedence::Callable)?;
                    }
                    self.push(')');
                }
                None => {}
            }
            if let Some(default) = bound_typevar.default_type(self.db) {
                self.push_str(" = ");
                self.print(default, Precedence::Callable)?;
            }
        }

        if !first {
            self.push(']');
        }
        Ok(())
    }

    fn print_parameters(
        &mut self,
        parameters: &super::signatures::Parameters<'db>,
    ) -> Result<(), PrintTypeError> {
        if parameters.is_top() {
            return Self::unsupported(UnsupportedTypeKind::TopCallableParameters);
        }
        if parameters.is_gradual() && parameters.len() == 2 {
            self.push_str("(...)");
            return Ok(());
        }

        let last_positional_only = parameters.iter().rposition(|parameter| {
            matches!(parameter.kind(), ParameterKind::PositionalOnly { .. })
        });
        let first_keyword_only = parameters
            .iter()
            .position(|parameter| matches!(parameter.kind(), ParameterKind::KeywordOnly { .. }));
        let has_variadic_before_keyword_only = first_keyword_only.is_some_and(|keyword_index| {
            parameters
                .iter()
                .take(keyword_index)
                .any(|parameter| matches!(parameter.kind(), ParameterKind::Variadic { .. }))
        });

        self.push('(');
        let mut first = true;
        for (index, parameter) in parameters.iter().enumerate() {
            if Some(index) == first_keyword_only && !has_variadic_before_keyword_only {
                self.write_separator(&mut first, ", ");
                self.push('*');
            }
            self.write_separator(&mut first, ", ");
            self.print_parameter(parameter)?;
            if Some(index) == last_positional_only {
                self.push_str(", /");
            }
        }
        self.push(')');
        Ok(())
    }

    fn print_parameter(&mut self, parameter: &Parameter<'db>) -> Result<(), PrintTypeError> {
        match parameter.kind() {
            ParameterKind::PositionalOnly {
                name: Some(name), ..
            }
            | ParameterKind::PositionalOrKeyword { name, .. }
            | ParameterKind::KeywordOnly { name, .. } => {
                self.push_str(name);
                self.push_str(": ");
            }
            ParameterKind::PositionalOnly { name: None, .. } => {}
            ParameterKind::Variadic { name } => {
                self.push('*');
                self.push_str(name);
                self.push_str(": ");
            }
            ParameterKind::KeywordVariadic { name } => {
                self.push_str("**");
                self.push_str(name);
                self.push_str(": ");
            }
        }
        self.print(parameter.annotated_type(), Precedence::Callable)?;
        if let Some(default) = parameter.default_type() {
            self.push_str(" = ");
            self.print_default(default)?;
        }
        Ok(())
    }

    fn print_default(&mut self, ty: Type<'db>) -> Result<(), PrintTypeError> {
        match ty {
            Type::LiteralValue(literal) => self.print_literal_value(literal),
            Type::NominalInstance(instance)
                if instance.class(self.db).known(self.db) == Some(KnownClass::NoneType) =>
            {
                self.push_str("None");
                Ok(())
            }
            _ => {
                self.push_str("...");
                Ok(())
            }
        }
    }

    fn print_literal_type(&mut self, literal: LiteralValueType<'db>) -> Result<(), PrintTypeError> {
        match literal.kind() {
            LiteralValueTypeKind::LiteralString => {
                self.print_intrinsic("typing", "LiteralString");
                Ok(())
            }
            _ => {
                self.print_intrinsic("typing", "Literal");
                self.push('[');
                self.print_literal_value(literal)?;
                self.push(']');
                Ok(())
            }
        }
    }

    fn print_literal_value(
        &mut self,
        literal: LiteralValueType<'db>,
    ) -> Result<(), PrintTypeError> {
        match literal.kind() {
            LiteralValueTypeKind::Int(value) => {
                let _ = write!(self.output, "{value}");
            }
            LiteralValueTypeKind::Bool(value) => {
                self.push_str(if value { "True" } else { "False" });
            }
            LiteralValueTypeKind::String(value) => {
                let _ = write!(
                    self.output,
                    "{}",
                    UnicodeEscape::with_preferred_quote(value.value(self.db), Quote::Double)
                        .str_repr(TripleQuotes::No)
                );
            }
            LiteralValueTypeKind::Bytes(value) => {
                let _ = write!(
                    self.output,
                    "{}",
                    AsciiEscape::with_preferred_quote(value.value(self.db), Quote::Double)
                        .bytes_repr(TripleQuotes::No)
                );
            }
            LiteralValueTypeKind::LiteralString => {
                self.push_str("...");
            }
            LiteralValueTypeKind::Enum(value) => {
                self.print_class_literal(value.enum_class(self.db))?;
                self.push('.');
                self.push_str(value.name(self.db));
            }
        }
        Ok(())
    }

    fn known_class(&mut self, class: KnownClass) -> Result<String, PrintTypeError> {
        if matches!(
            class,
            KnownClass::NamedTupleFallback
                | KnownClass::NamedTupleLike
                | KnownClass::TypedDictFallback
                | KnownClass::ConstraintSet
                | KnownClass::GenericContext
                | KnownClass::Specialization
        ) {
            return Self::unsupported(UnsupportedTypeKind::InternalNominal);
        }
        Ok(Self::intrinsic(
            class.canonical_module(self.db).as_str(),
            class.name(self.db),
        ))
    }

    fn push(&mut self, character: char) {
        self.output.push(character);
    }

    fn push_str(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn write_separator(&mut self, first: &mut bool, separator: &str) {
        if *first {
            *first = false;
        } else {
            self.push_str(separator);
        }
    }

    fn print_intrinsic(&mut self, module: &str, name: &str) {
        let _ = write!(self.output, "{module}.{name}");
    }

    fn intrinsic(module: &str, name: &str) -> String {
        format!("{module}.{name}")
    }

    fn definition_name(
        &mut self,
        definition: Definition<'db>,
        name: &str,
    ) -> Result<String, PrintTypeError> {
        self.qualified_definition_name(definition, None, name)
            .ok_or_else(|| PrintTypeError::UnresolvedName {
                name: name.to_string(),
            })
    }

    fn qualified_definition_name(
        &self,
        definition: Definition<'db>,
        function: Option<FunctionType<'db>>,
        name: &str,
    ) -> Option<String> {
        let file = definition.file(self.db);
        let module = file_to_module(self.db, file)?;
        let parsed = parsed_module(self.db, file).load(self.db);
        let index = semantic_index(self.db, file);
        let mut components = Vec::new();

        for (_, ancestor_scope) in index.ancestor_scopes(definition.file_scope(self.db)) {
            match ancestor_scope.kind() {
                ScopeKind::Class => {
                    if let Some(class) = ancestor_scope.node().as_class() {
                        let class = class.node(&parsed);
                        components.push(self.definition_name_component(
                            index.expect_single_definition(class),
                            None,
                            class.name.as_str(),
                        ));
                    }
                }
                ScopeKind::Function => {
                    if let Some(function) = ancestor_scope.node().as_function() {
                        let function = function.node(&parsed);
                        components.push(self.definition_name_component(
                            index.expect_single_definition(function),
                            None,
                            function.name.as_str(),
                        ));
                    }
                }
                _ => {}
            }
        }
        components.push(module.name(self.db).to_string());
        components.reverse();
        components.push(self.definition_name_component(definition, function, name));
        Some(components.join("."))
    }

    fn definition_name_component(
        &self,
        definition: Definition<'db>,
        function: Option<FunctionType<'db>>,
        name: &str,
    ) -> String {
        let place = definition.place(self.db);
        let definitions = use_def_map(self.db, definition.scope(self.db))
            .all_definitions_with_usage()
            .filter_map(|(_, state, _)| state.definition())
            .filter(|other| other.place(self.db) == place)
            .collect::<Vec<_>>();
        let ambiguous = definitions.iter().any(|other| {
            *other != definition
                && function.is_none_or(|function| !function.contains_definition(self.db, *other))
        });

        if !ambiguous {
            return name.to_string();
        }

        let ordinal = definitions
            .iter()
            .position(|other| *other == definition)
            .expect("definition should be present in its scope's use-def map")
            + 1;
        format!("{name}@{ordinal}")
    }

    fn function_is_async(&self, definition: Definition<'db>) -> bool {
        let DefinitionKind::Function(function) = definition.kind(self.db) else {
            return false;
        };
        let parsed = parsed_module(self.db, definition.file(self.db)).load(self.db);
        function.node(&parsed).is_async
    }

    fn unsupported<T>(kind: UnsupportedTypeKind) -> Result<T, PrintTypeError> {
        Err(PrintTypeError::UnsupportedType { kind })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{TestDbBuilder, setup_db};
    use crate::place::global_symbol;
    use crate::types::generics::typing_self;
    use crate::types::set_theoretic::builder::IntersectionBuilder;
    use crate::types::subclass_of::SubclassOfType;
    use crate::types::{
        InternedType, KnownClass, Parameters, Signature, SpecialFormType, Type, UnionType,
        todo_type,
    };
    use insta::assert_snapshot;
    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::name::Name;

    fn printed(result: Result<String, PrintTypeError>) -> String {
        result.unwrap_or_else(|error| format!("Failed with {error}"))
    }

    #[test]
    fn typing_self_is_not_treated_as_an_emitted_binder() {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", "class C: ...\n")
            .build()
            .expect("valid test database");
        let file = system_path_to_file(&db, "/src/foo.py").expect("test file exists");
        let Type::ClassLiteral(ClassLiteral::Static(class)) =
            global_symbol(&db, file, "C").place.expect_type()
        else {
            panic!("C should be a class literal");
        };
        let self_typevar = typing_self(
            &db,
            class.body_scope(&db),
            Some(class.definition(&db)),
            class.into(),
        )
        .expect("class has a typing.Self variable");
        let context = GenericContext::from_typevar_instances(&db, [self_typevar]);
        let callable = Type::single_callable(
            &db,
            Signature::new_generic(
                Some(context),
                Parameters::empty(),
                Type::TypeVar(self_typevar),
            ),
        );

        assert_snapshot!(printed(print_type(&db, callable)), @"() -> typing.Self");
    }

    #[test]
    fn literal_string_default_with_unknown_value_uses_ellipsis() {
        let db = setup_db();
        let callable = Type::single_callable(
            &db,
            Signature::new(
                Parameters::new(
                    &db,
                    [Parameter::positional_or_keyword(Name::new_static("value"))
                        .with_annotated_type(Type::literal_string())
                        .with_default_type(Type::literal_string())],
                ),
                Type::none(&db),
            ),
        );

        assert_snapshot!(
            printed(print_type(&db, callable)),
            @"(value: typing.LiteralString = ...) -> None"
        );
    }

    #[test]
    fn public_numeric_types_are_normalized_in_generics() {
        let db = setup_db();
        let generic =
            KnownClass::List.to_specialized_instance(&db, &[KnownClass::Float.to_instance(&db)]);

        assert_snapshot!(printed(print_type(&db, generic)), @"builtins.list[builtins.float]");
    }

    #[test]
    fn public_numeric_types_are_normalized_in_callables() {
        let db = setup_db();
        let callable = Type::single_callable(
            &db,
            Signature::new(
                Parameters::new(
                    &db,
                    [Parameter::positional_only(None)
                        .with_annotated_type(KnownClass::Float.to_instance(&db))],
                ),
                KnownClass::Complex.to_instance(&db),
            ),
        );

        assert_snapshot!(
            printed(print_type(&db, callable)),
            @"(builtins.float, /) -> builtins.complex"
        );
    }

    #[test]
    fn special_form_value_uses_typeof() {
        let db = setup_db();
        assert_snapshot!(
            printed(print_type(
                &db,
                Type::SpecialForm(SpecialFormType::Literal)
            )),
            @"ty_extensions.TypeOf[typing.Literal]"
        );
    }

    #[test]
    fn annotated_uses_the_precedence_of_its_bare_type() {
        let db = setup_db();
        let union = UnionType::from_two_elements(
            &db,
            KnownClass::Int.to_instance(&db),
            KnownClass::Str.to_instance(&db),
        );
        let annotated =
            Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(&db, union)));
        let mut printer = PrintType {
            db: &db,
            output: String::new(),
            active: FxHashSet::default(),
            binders: Vec::new(),
        };

        printer
            .print(annotated, Precedence::Intersection)
            .expect("annotated union should be printable");
        assert_snapshot!(printer.output, @"(builtins.int | builtins.str)");
    }

    #[test]
    fn provide_type_omits_only_direct_synthesized_protocol_conjuncts() {
        let db = setup_db();
        let synthesized = Type::protocol_with_readonly_members(
            &db,
            [("member", KnownClass::Int.to_instance(&db))],
        );
        let other_synthesized = Type::protocol_with_readonly_members(
            &db,
            [("other", KnownClass::Str.to_instance(&db))],
        );
        let positive = IntersectionBuilder::new(&db)
            .add_positive(Type::AlwaysTruthy)
            .add_positive(synthesized)
            .build();
        let negative = IntersectionBuilder::new(&db)
            .add_positive(Type::AlwaysTruthy)
            .add_negative(synthesized)
            .build();
        let only_synthesized = IntersectionBuilder::new(&db)
            .add_positive(synthesized)
            .add_positive(other_synthesized)
            .build();

        let intersection_outputs = [positive, negative]
            .map(|ty| printed(print_type(&db, ty)))
            .join("\n");
        assert_snapshot!(intersection_outputs, @r"
        ty_extensions.AlwaysTruthy
        ty_extensions.AlwaysTruthy
        ");
        assert!(matches!(only_synthesized, Type::Intersection(_)));

        let nested = KnownClass::List.to_specialized_instance(&db, &[synthesized]);
        let errors = [synthesized, only_synthesized, nested]
            .map(|ty| printed(print_type(&db, ty)))
            .join("\n");
        assert_snapshot!(errors, @r"
        Failed with type `synthesized protocol` cannot be printed
        Failed with type `synthesized protocol` cannot be printed
        Failed with type `synthesized protocol` cannot be printed
        ");
    }

    #[test]
    fn intersection_and_negation_use_python_precedence() {
        let db = setup_db();
        let ty = IntersectionBuilder::new(&db)
            .add_positive(Type::AlwaysTruthy)
            .add_negative(KnownClass::Int.to_instance(&db))
            .build();

        assert_snapshot!(
            printed(print_type(&db, ty)),
            @"ty_extensions.AlwaysTruthy & ~builtins.int"
        );
    }

    #[test]
    fn internal_types_are_rejected() {
        let db = setup_db();

        let errors = [
            Type::function_like_callable(&db, Signature::new(Parameters::empty(), Type::unknown())),
            KnownClass::ConstraintSet.to_instance(&db),
        ]
        .map(|ty| printed(print_type(&db, ty)))
        .join("\n");
        assert_snapshot!(errors, @r"
        Failed with type `non-regular anonymous callable` cannot be printed
        Failed with type `internal nominal type` cannot be printed
        ");
    }

    #[test]
    fn todo_types_are_normalized_to_unknown() {
        let db = setup_db();

        let todo_types = [
            todo_type!("type expression printing test"),
            Type::Dynamic(DynamicType::TodoUnpack),
            Type::Dynamic(DynamicType::TodoStarredExpression),
            Type::Dynamic(DynamicType::TodoTypeVarTuple),
        ]
        .map(|ty| printed(print_type(&db, ty)))
        .join("\n");
        assert_snapshot!(todo_types, @r"
        Unknown
        Unknown
        Unknown
        Unknown
        ");
    }

    #[test]
    fn internal_dynamic_types_are_normalized_to_any() {
        let db = setup_db();
        let generic_context = GenericContext::from_typevar_instances(&db, []);

        let dynamic_types = [
            DynamicType::UnknownGeneric(generic_context),
            DynamicType::UnspecializedTypeVar,
            DynamicType::InvalidConcatenateUnknown,
            DynamicType::AmbiguousOverload,
        ]
        .map(|dynamic| printed(print_type(&db, Type::Dynamic(dynamic))))
        .join("\n");
        assert_snapshot!(dynamic_types, @r"
        typing.Any
        typing.Any
        typing.Any
        typing.Any
        ");
    }

    #[test]
    fn todo_subclass_types_are_normalized_to_unknown() {
        let db = setup_db();
        let todo_subclasses = [
            DynamicType::TodoUnpack,
            DynamicType::TodoStarredExpression,
            DynamicType::TodoTypeVarTuple,
        ]
        .map(|dynamic| printed(print_type(&db, SubclassOfType::from(&db, dynamic))))
        .join("\n");
        assert_snapshot!(todo_subclasses, @r"
        builtins.type[Unknown]
        builtins.type[Unknown]
        builtins.type[Unknown]
        ");
    }

    #[test]
    fn divergent_uses_the_explicit_extension() {
        let db = setup_db();
        let divergent = Type::divergent(salsa::plumbing::Id::from_bits(1));

        assert_snapshot!(printed(print_type(&db, divergent)), @"Divergent");
    }

    #[test]
    fn deeply_nested_callable_types_are_not_truncated() {
        let db = setup_db();
        let mut ty = KnownClass::Int.to_instance(&db);
        for _ in 0..128 {
            ty = Type::single_callable(&db, Signature::new(Parameters::empty(), ty));
        }

        let printed = print_type(&db, ty).expect("nested type is printable");
        assert_eq!(printed.matches("() -> ").count(), 128);
        assert!(printed.ends_with("builtins.int"));
    }
}
