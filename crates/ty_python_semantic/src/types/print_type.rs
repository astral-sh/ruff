//! Exact, machine-readable printing of semantic types.
//!
//! Type printing is deliberately separate from ordinary type display. Display is optimized for
//! people and may abbreviate or truncate a type. [`print_type`] is exact: it either prints the
//! complete type or returns [`PrintTypeError`]. [`print_type_for_provide_type`] uses the same
//! traversal but applies the endpoint's documented normalizations while printing.
//!
//! Aliases are always preserved as references. Their values are never visited. This both preserves
//! the type identity selected by inference and makes named recursive aliases terminate naturally.
//!
//! # Canonical exact spellings
//!
//! Some semantic types have an internal representation that differs from their canonical source
//! spelling. In particular, ty represents the numeric-tower annotation `float` as the union of
//! `int` and an exact-float instance, and `complex` as the union of `int`, exact-float, and
//! exact-complex instances. [`print_type`] recognizes those unions, including when they are part of
//! a larger union, and prints the semantics-preserving canonical `float` or `complex` spelling.
//! This is not a provide-type approximation.
//!
//! # Provide-type normalizations
//!
//! Provide-type output favors public, parseable annotations over extensions that expose ty's
//! precise internal representation. It performs the following exhaustive set of normalizations
//! during printing, without constructing an intermediate promoted [`Type`]:
//!
//! - Exact `float` and `complex` instances are printed as `float` and `complex`, respectively.
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
//! may contain dots in every name position, including after `def`.
//!
//! ```text
//! type          ::= union
//! union         ::= intersection (" | " intersection)*
//! intersection  ::= unary (" & " unary)*
//! unary         ::= "~" unary | primary
//! primary       ::= name
//!                 | free_typevar
//!                 | "None"
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
//! name          ::= identifier ("." identifier)*
//! free_typevar  ::= identifier ["." ("args" | "kwargs")] ["@" name]
//! ```
//!
//! Python syntax and precedence are used where possible. The experimental extensions are
//! callable expressions, overload groups, exact class-object `TypeOf` expressions, exact
//! `JustFloat` and `JustComplex` instance types, scoped free type variables, truthiness types,
//! intersections, and negation. Their precedence is `~`, then `&`, then `|`. The printer inserts
//! parentheses whenever a nested expression would otherwise change meaning.
//!
//! Named classes, aliases, functions, type variables, and `NewType`s are resolved by semantic
//! declaration identity. Printing fails if that identity has no name or if multiple declarations
//! have the same lexical path. Except for the provide-type omission described above, anonymous
//! structural and inference-only types do not acquire synthetic names; they are unsupported.

use std::fmt::Write as _;

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
    KnownInstanceType, KnownUnion, LiteralValueType, LiteralValueTypeKind, ParameterKind,
    SubclassOfInner, Type, TypeAliasType, TypeVarBoundOrConstraints, TypeVarKind, TypedDictType,
};
use crate::Db;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PrintTypeError {
    #[error("type `{kind}` cannot be printed")]
    UnsupportedType { kind: &'static str },
    #[error("anonymous recursive type")]
    RecursiveType,
    #[error("name `{name}` is ambiguous")]
    AmbiguousName { name: String },
    #[error("name `{name}` cannot be resolved")]
    UnresolvedName { name: String },
}

/// Prints `ty` exactly.
pub fn print_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Result<String, PrintTypeError> {
    print_type_with_normalization(db, ty, Normalization::Exact)
}

/// Prints the endpoint-specific public representation of `ty`.
///
/// This applies the documented, potentially lossy provide-type normalizations. Use [`print_type`]
/// when the caller requires an exact, general-purpose type representation.
pub fn print_type_for_provide_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Result<String, PrintTypeError> {
    print_type_with_normalization(db, ty, Normalization::ProvideType)
}

fn print_type_with_normalization<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    normalization: Normalization,
) -> Result<String, PrintTypeError> {
    let mut printer = PrintType {
        db,
        normalization,
        output: String::new(),
        active: FxHashSet::default(),
        binders: Vec::new(),
    };
    printer.print(ty, Precedence::Callable)?;
    Ok(printer.output)
}

struct PrintType<'db> {
    db: &'db dyn Db,
    normalization: Normalization,
    output: String,
    active: FxHashSet<Type<'db>>,
    binders: Vec<GenericContext<'db>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Normalization {
    Exact,
    ProvideType,
}

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

        let parenthesized = Self::precedence(ty) < parent_precedence;
        if parenthesized {
            self.output.push('(');
        }
        self.print_inner(ty)?;
        self.active.remove(&ty);

        if parenthesized {
            self.output.push(')');
        }
        Ok(())
    }

    const fn precedence(ty: Type<'db>) -> Precedence {
        match ty {
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::Callable(_) => {
                Precedence::Callable
            }
            Type::Union(_) => Precedence::Union,
            Type::Intersection(_) => Precedence::Intersection,
            _ => Precedence::Primary,
        }
    }

    fn print_inner(&mut self, ty: Type<'db>) -> Result<(), PrintTypeError> {
        match ty {
            Type::Dynamic(DynamicType::Any) => {
                self.print_intrinsic("typing", "Any");
            }
            Type::Dynamic(DynamicType::Unknown) => self.output.push_str("Unknown"),
            Type::Dynamic(
                DynamicType::UnknownGeneric(_)
                | DynamicType::UnspecializedTypeVar
                | DynamicType::InvalidConcatenateUnknown
                | DynamicType::AmbiguousOverload
                | DynamicType::Todo(_)
                | DynamicType::TodoUnpack
                | DynamicType::TodoStarredExpression
                | DynamicType::TodoTypeVarTuple,
            ) => return Self::unsupported("internal dynamic type"),
            Type::Divergent(_) => return Self::unsupported("divergent inference type"),
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
            Type::KnownBoundMethod(_) => return Self::unsupported("internal bound method"),
            Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_) => return Self::unsupported("internal callable type"),
            Type::Callable(callable) => {
                if callable.kind(self.db) != CallableTypeKind::Regular {
                    return Self::unsupported("non-regular anonymous callable");
                }
                self.print_callable(callable.signatures(self.db), None, false)?;
            }
            Type::ModuleLiteral(_) => return Self::unsupported("module type"),
            Type::ClassLiteral(class) => {
                self.print_intrinsic("ty_extensions", "TypeOf");
                self.output.push('[');
                self.print_class_literal(class)?;
                self.output.push(']');
            }
            Type::GenericAlias(alias) => {
                self.print_intrinsic("ty_extensions", "TypeOf");
                self.output.push('[');
                self.print_generic_alias(alias)?;
                self.output.push(']');
            }
            Type::SubclassOf(subclass) => {
                self.print_intrinsic("builtins", "type");
                self.output.push('[');
                match subclass.subclass_of() {
                    SubclassOfInner::Class(class) => self.print_instance_class_type(class)?,
                    SubclassOfInner::Dynamic(DynamicType::Any) => {
                        self.print_intrinsic("typing", "Any");
                    }
                    SubclassOfInner::Dynamic(DynamicType::Unknown) => {
                        self.output.push_str("Unknown");
                    }
                    SubclassOfInner::Dynamic(_) => {
                        return Self::unsupported("internal dynamic type");
                    }
                    SubclassOfInner::TypeVar(typevar) => self.print_bound_typevar(typevar)?,
                }
                self.output.push(']');
            }
            Type::NominalInstance(instance) => {
                let class = instance.class(self.db);
                if class.known(self.db) == Some(KnownClass::NoneType) {
                    self.output.push_str("None");
                } else {
                    self.print_instance_class_type(class)?;
                }
            }
            Type::ProtocolInstance(protocol) => {
                let Some(instance) = protocol.to_nominal_instance() else {
                    return Self::unsupported("synthesized protocol");
                };
                self.print_instance_class_type(instance.class(self.db))?;
            }
            Type::KnownInstance(instance @ KnownInstanceType::TypeAliasType(_))
                if self.normalization == Normalization::ProvideType =>
            {
                let name = self.known_class(instance.class(self.db))?;
                self.output.push_str(&name);
            }
            Type::SpecialForm(_) | Type::KnownInstance(_) => {
                return Self::unsupported("runtime typing object");
            }
            Type::PropertyInstance(_) => return Self::unsupported("property instance"),
            Type::Union(union) => {
                let numeric_class = |ty: Type<'db>| {
                    ty.as_nominal_instance()
                        .and_then(|instance| instance.known_class(self.db))
                };
                let mut has_int = false;
                let mut has_float = false;
                let mut has_complex = false;
                for element in union.elements(self.db) {
                    match numeric_class(*element) {
                        Some(KnownClass::Int) => has_int = true,
                        Some(KnownClass::Float) => has_float = true,
                        Some(KnownClass::Complex) => has_complex = true,
                        _ => {}
                    }
                }
                let shorthand = if has_int && has_float && has_complex {
                    Some(KnownUnion::Complex)
                } else if has_int && has_float {
                    Some(KnownUnion::Float)
                } else {
                    None
                };

                let mut first = true;
                let mut shorthand_emitted = false;
                for element in union.elements(self.db) {
                    if let Some(known) = shorthand
                        && known.contains(numeric_class(*element))
                    {
                        if shorthand_emitted {
                            continue;
                        }
                        shorthand_emitted = true;
                        self.write_separator(&mut first, " | ");
                        self.print_intrinsic("builtins", known.name());
                    } else {
                        self.write_separator(&mut first, " | ");
                        self.print(*element, Precedence::Union)?;
                    }
                }
            }
            Type::Intersection(intersection) => {
                let normalization = self.normalization;
                let omit =
                    |ty: &Type<'db>| Self::should_omit_intersection_conjunct(normalization, *ty);
                let positive_count = intersection
                    .positive(self.db)
                    .iter()
                    .filter(|ty| !omit(ty))
                    .count();
                let negative_count = intersection
                    .negative(self.db)
                    .iter()
                    .filter(|ty| !omit(ty))
                    .count();

                if positive_count + negative_count == 0 {
                    return Self::unsupported("synthesized protocol");
                }
                let mut first = true;
                for element in intersection.positive(self.db).iter().filter(|ty| !omit(ty)) {
                    self.write_separator(&mut first, " & ");
                    self.print(*element, Precedence::Intersection)?;
                }
                for element in intersection.negative(self.db).iter().filter(|ty| !omit(ty)) {
                    self.write_separator(&mut first, " & ");
                    self.output.push('~');
                    self.print(*element, Precedence::Unary)?;
                }
            }
            Type::EnumComplement(_) => return Self::unsupported("enum complement"),
            Type::AlwaysTruthy => {
                self.print_intrinsic("ty_extensions", "AlwaysTruthy");
            }
            Type::AlwaysFalsy => {
                self.print_intrinsic("ty_extensions", "AlwaysFalsy");
            }
            Type::LiteralValue(literal) => self.print_literal_type(literal)?,
            Type::TypeVar(typevar) => self.print_bound_typevar(typevar)?,
            Type::BoundSuper(_) => return Self::unsupported("bound super type"),
            Type::TypeIs(_) => return Self::unsupported("TypeIs narrowing type"),
            Type::TypeGuard(_) => return Self::unsupported("TypeGuard narrowing type"),
            Type::TypeForm(_) => return Self::unsupported("type form"),
            Type::TypedDict(TypedDictType::Class(class)) => {
                self.print_class_type(class)?;
            }
            Type::TypedDict(TypedDictType::Synthesized(_)) => {
                return Self::unsupported("synthesized TypedDict");
            }
            Type::TypeAlias(alias) => self.print_alias(alias)?,
            Type::NewTypeInstance(newtype) => {
                let name =
                    self.definition_name(newtype.definition(self.db), newtype.name(self.db))?;
                self.output.push_str(&name);
            }
        }

        Ok(())
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
        self.output.push_str(&name);
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

    fn print_instance_class_type(&mut self, class: ClassType<'db>) -> Result<(), PrintTypeError> {
        if self.normalization == Normalization::ProvideType
            && matches!(
                class.known(self.db),
                Some(KnownClass::Float | KnownClass::Complex)
            )
        {
            return self.print_class_type(class);
        }

        let exact_name = match class.known(self.db) {
            Some(KnownClass::Float) => Some("JustFloat"),
            Some(KnownClass::Complex) => Some("JustComplex"),
            _ => None,
        };
        let Some(exact_name) = exact_name else {
            return self.print_class_type(class);
        };

        self.print_intrinsic("ty_extensions", exact_name);
        Ok(())
    }

    fn should_omit_intersection_conjunct(normalization: Normalization, ty: Type<'db>) -> bool {
        normalization == Normalization::ProvideType
            && matches!(
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
            return Self::unsupported("materialized generic specialization");
        }

        if known == Some(KnownClass::Tuple)
            && let Some(tuple) = specialization.tuple(self.db)
        {
            return self.print_tuple(name, tuple);
        }

        self.output.push('[');
        for (index, ty) in specialization.types(self.db).iter().enumerate() {
            if index > 0 {
                self.output.push_str(", ");
            }
            self.print(*ty, Precedence::Callable)?;
        }
        self.output.push(']');
        Ok(())
    }

    fn print_tuple(&mut self, name: &str, tuple: &TupleSpec<'db>) -> Result<(), PrintTypeError> {
        self.output.push('[');
        let mut first = true;
        match tuple {
            TupleSpec::Fixed(fixed) => {
                for element in fixed.iter_all_elements() {
                    self.write_separator(&mut first, ", ");
                    self.print(element, Precedence::Callable)?;
                }
                if first {
                    self.output.push_str("()");
                }
            }
            TupleSpec::Variable(variable) => {
                for element in variable.iter_prefix_elements() {
                    self.write_separator(&mut first, ", ");
                    self.print(element, Precedence::Callable)?;
                }
                if first && variable.suffix_elements().is_empty() {
                    self.print(variable.variable(), Precedence::Callable)?;
                    self.output.push_str(", ...");
                } else {
                    self.write_separator(&mut first, ", ");
                    self.output.push('*');
                    self.output.push_str(name);
                    self.output.push('[');
                    self.print(variable.variable(), Precedence::Callable)?;
                    self.output.push_str(", ...]");
                    for element in variable.iter_suffix_elements() {
                        self.write_separator(&mut first, ", ");
                        self.print(element, Precedence::Callable)?;
                    }
                }
            }
        }
        self.output.push(']');
        Ok(())
    }

    fn print_alias(&mut self, alias: TypeAliasType<'db>) -> Result<(), PrintTypeError> {
        let name = self.definition_name(alias.definition(self.db), alias.name(self.db))?;
        self.output.push_str(&name);
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
            self.output.push_str(typevar.name(self.db));
            if let Some(attr) = attr {
                let _ = write!(self.output, ".{attr}");
            }
        } else {
            if typevar.freshness(self.db).value() != 0 {
                return Self::unsupported("fresh inference type variable");
            }

            let Some(binding_definition) = typevar.binding_context(self.db).definition() else {
                return Self::unsupported("synthetic type variable");
            };
            self.check_ambiguity(binding_definition, None)?;
            let definition = typevar
                .typevar(self.db)
                .definition(self.db)
                .unwrap_or(binding_definition);
            self.check_ambiguity(definition, None)?;
            let name = attr.map_or_else(
                || typevar.name(self.db).to_string(),
                |attr| format!("{}.{attr}", typevar.name(self.db)),
            );
            let qualified_name = binding_definition
                .name(self.db)
                .and_then(|binding_name| {
                    self.qualified_definition_name(binding_definition, &binding_name)
                })
                .map(|binding_name| format!("{name}@{binding_name}"));
            let qualified_name = qualified_name.ok_or(PrintTypeError::UnresolvedName { name })?;
            self.output.push_str(&qualified_name);
        }
        Ok(())
    }

    fn print_named_callable(
        &mut self,
        function: FunctionType<'db>,
        signatures: &CallableSignature<'db>,
    ) -> Result<(), PrintTypeError> {
        let definition = function.definition(self.db);
        self.check_ambiguity(definition, Some(function))?;
        let name = self
            .qualified_definition_name(definition, function.name(self.db))
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
            return Self::unsupported("empty callable");
        }

        let overloaded = signatures.overloads.len() > 1;
        if overloaded {
            self.output.push_str("Overloads[");
        }
        for (index, signature) in signatures.overloads.iter().enumerate() {
            if index > 0 {
                self.output.push_str(", ");
            }
            let signature_is_async = name.is_some()
                && signature
                    .definition
                    .map_or(is_async, |definition| self.function_is_async(definition));
            self.print_signature(signature, name, signature_is_async)?;
        }
        if overloaded {
            self.output.push(']');
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
            self.output.push_str("async ");
        }
        if let Some(name) = name {
            self.output.push_str("def ");
            self.output.push_str(name);
        }
        self.print_generic_context(signature.generic_context)?;
        self.print_parameters(signature.parameters())?;
        self.output.push_str(" -> ");
        let return_ty = if is_async {
            self.declared_async_return_type(signature.return_ty)
        } else {
            signature.return_ty
        };
        self.print(return_ty, Precedence::Callable)?;
        if name.is_some() {
            self.output.push_str(": ...");
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
                self.output.push('[');
            } else {
                self.output.push_str(", ");
            }
            first = false;
            if typevar.is_paramspec(self.db) {
                self.output.push_str("**");
            }
            self.output.push_str(typevar.name(self.db));
            match typevar.bound_or_constraints(self.db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    self.output.push_str(": ");
                    self.print(bound, Precedence::Callable)?;
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    self.output.push_str(": (");
                    for (index, constraint) in constraints.elements(self.db).iter().enumerate() {
                        if index > 0 {
                            self.output.push_str(", ");
                        }
                        self.print(*constraint, Precedence::Callable)?;
                    }
                    self.output.push(')');
                }
                None => {}
            }
            if let Some(default) = bound_typevar.default_type(self.db) {
                self.output.push_str(" = ");
                self.print(default, Precedence::Callable)?;
            }
        }

        if !first {
            self.output.push(']');
        }
        Ok(())
    }

    fn print_parameters(
        &mut self,
        parameters: &super::signatures::Parameters<'db>,
    ) -> Result<(), PrintTypeError> {
        if parameters.is_top() {
            return Self::unsupported("top callable parameters");
        }
        if parameters.is_gradual() && parameters.len() == 2 {
            self.output.push_str("(...)");
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

        self.output.push('(');
        let mut first = true;
        for (index, parameter) in parameters.iter().enumerate() {
            if Some(index) == first_keyword_only && !has_variadic_before_keyword_only {
                self.write_separator(&mut first, ", ");
                self.output.push('*');
            }
            self.write_separator(&mut first, ", ");
            self.print_parameter(parameter)?;
            if Some(index) == last_positional_only {
                self.output.push_str(", /");
            }
        }
        self.output.push(')');
        Ok(())
    }

    fn print_parameter(&mut self, parameter: &Parameter<'db>) -> Result<(), PrintTypeError> {
        match parameter.kind() {
            ParameterKind::PositionalOnly {
                name: Some(name), ..
            }
            | ParameterKind::PositionalOrKeyword { name, .. }
            | ParameterKind::KeywordOnly { name, .. } => {
                self.output.push_str(name);
                self.output.push_str(": ");
            }
            ParameterKind::PositionalOnly { name: None, .. } => {}
            ParameterKind::Variadic { name } => {
                self.output.push('*');
                self.output.push_str(name);
                self.output.push_str(": ");
            }
            ParameterKind::KeywordVariadic { name } => {
                self.output.push_str("**");
                self.output.push_str(name);
                self.output.push_str(": ");
            }
        }
        self.print(parameter.annotated_type(), Precedence::Callable)?;
        if let Some(default) = parameter.default_type() {
            self.output.push_str(" = ");
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
                self.output.push_str("None");
                Ok(())
            }
            _ => {
                self.output.push_str("...");
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
                self.output.push('[');
                self.print_literal_value(literal)?;
                self.output.push(']');
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
                self.output.push_str(if value { "True" } else { "False" });
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
                return Self::unsupported("non-literal LiteralString value");
            }
            LiteralValueTypeKind::Enum(value) => {
                self.print_class_literal(value.enum_class(self.db))?;
                self.output.push('.');
                self.output.push_str(value.name(self.db));
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
            return Self::unsupported("internal nominal type");
        }
        Ok(Self::intrinsic(
            class.canonical_module(self.db).as_str(),
            class.name(self.db),
        ))
    }

    fn write_separator(&mut self, first: &mut bool, separator: &str) {
        if *first {
            *first = false;
        } else {
            self.output.push_str(separator);
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
        self.check_ambiguity(definition, None)?;
        self.qualified_definition_name(definition, name)
            .ok_or_else(|| PrintTypeError::UnresolvedName {
                name: name.to_string(),
            })
    }

    fn qualified_definition_name(&self, definition: Definition<'db>, name: &str) -> Option<String> {
        let file = definition.file(self.db);
        let module = file_to_module(self.db, file)?;
        let parsed = parsed_module(self.db, file).load(self.db);
        let index = semantic_index(self.db, file);
        let mut components = Vec::new();

        for (_, ancestor_scope) in index.ancestor_scopes(definition.file_scope(self.db)) {
            match ancestor_scope.kind() {
                ScopeKind::Class => {
                    if let Some(class) = ancestor_scope.node().as_class() {
                        components.push(class.node(&parsed).name.as_str());
                    }
                }
                ScopeKind::Function => {
                    if let Some(function) = ancestor_scope.node().as_function() {
                        components.push(function.node(&parsed).name.as_str());
                    }
                }
                _ => {}
            }
        }
        components.push(module.name(self.db).as_str());
        components.reverse();
        components.push(name);
        Some(components.join("."))
    }

    fn check_ambiguity(
        &self,
        definition: Definition<'db>,
        function: Option<FunctionType<'db>>,
    ) -> Result<(), PrintTypeError> {
        self.check_definition_ambiguity(definition, function)?;

        let file = definition.file(self.db);
        let parsed = parsed_module(self.db, file).load(self.db);
        let index = semantic_index(self.db, file);
        for (_, ancestor_scope) in index.ancestor_scopes(definition.file_scope(self.db)) {
            let definitions = match ancestor_scope.kind() {
                ScopeKind::Class => ancestor_scope
                    .node()
                    .as_class()
                    .map(|class| index.definitions(class.node(&parsed))),
                ScopeKind::Function => ancestor_scope
                    .node()
                    .as_function()
                    .map(|function| index.definitions(function.node(&parsed))),
                _ => None,
            };
            if let Some(definitions) = definitions {
                for ancestor in definitions {
                    self.check_definition_ambiguity(*ancestor, None)?;
                }
            }
        }

        Ok(())
    }

    fn check_definition_ambiguity(
        &self,
        definition: Definition<'db>,
        function: Option<FunctionType<'db>>,
    ) -> Result<(), PrintTypeError> {
        let place = definition.place(self.db);
        let ambiguous = use_def_map(self.db, definition.scope(self.db))
            .all_definitions_with_usage()
            .filter_map(|(_, state, _)| state.definition())
            .filter(|other| other.place(self.db) == place && *other != definition)
            .any(|other| {
                function.is_none_or(|function| !function.contains_definition(self.db, other))
            });

        if ambiguous {
            Err(PrintTypeError::AmbiguousName {
                name: definition
                    .name(self.db)
                    .unwrap_or_else(|| "<unnamed>".to_string()),
            })
        } else {
            Ok(())
        }
    }

    fn function_is_async(&self, definition: Definition<'db>) -> bool {
        let DefinitionKind::Function(function) = definition.kind(self.db) else {
            return false;
        };
        let parsed = parsed_module(self.db, definition.file(self.db)).load(self.db);
        function.node(&parsed).is_async
    }

    fn unsupported<T>(kind: &'static str) -> Result<T, PrintTypeError> {
        Err(PrintTypeError::UnsupportedType { kind })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{TestDbBuilder, setup_db};
    use crate::place::global_symbol;
    use crate::types::callable::CallableType;
    use crate::types::generics::typing_self;
    use crate::types::set_theoretic::builder::IntersectionBuilder;
    use crate::types::typed_dict::{
        SynthesizedTypedDictKind, SynthesizedTypedDictType, TypedDictOpenness, TypedDictSchema,
    };
    use crate::types::{KnownClass, Parameters, Signature, SpecialFormType, Type, todo_type};
    use ruff_db::files::system_path_to_file;

    fn print_symbol(source: &str, symbol: &str) -> Result<String, PrintTypeError> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", source)
            .build()
            .expect("valid test database");
        let file = system_path_to_file(&db, "/src/foo.py").expect("test file exists");
        let ty = global_symbol(&db, file, symbol).place.expect_type();
        print_type(&db, ty)
    }

    fn print_provided_symbol(source: &str, symbol: &str) -> Result<String, PrintTypeError> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", source)
            .build()
            .expect("valid test database");
        let file = system_path_to_file(&db, "/src/foo.py").expect("test file exists");
        let ty = global_symbol(&db, file, symbol).place.expect_type();
        print_type_for_provide_type(&db, ty)
    }

    #[test]
    fn aliases_are_preserved_including_named_recursion() {
        assert_eq!(
            print_symbol(
                r#"
type Tree = int | list[Tree]
tree: Tree
"#,
                "tree",
            ),
            Ok("foo.Tree".to_string())
        );
        assert_eq!(
            print_symbol(
                r#"
type Box[T] = list[T]
box: Box[int]
"#,
                "box",
            ),
            Ok("foo.Box[builtins.int]".to_string())
        );
    }

    #[test]
    fn newtype_instances_use_their_declaration_name() {
        assert_eq!(
            print_symbol(
                r#"
from typing import NewType
UserId = NewType("UserId", int)
user = UserId(1)
"#,
                "user",
            ),
            Ok("foo.UserId".to_string())
        );
    }

    #[test]
    fn class_objects_use_the_exact_typeof_extension() {
        assert_eq!(
            print_symbol(
                r#"
class C: ...
value = C
"#,
                "value",
            ),
            Ok("ty_extensions.TypeOf[foo.C]".to_string())
        );
    }

    #[test]
    fn generic_binders_use_local_type_variable_names() {
        assert_eq!(
            print_symbol(
                r#"
def identity[T](value: T) -> T:
    return value
"#,
                "identity",
            ),
            Ok("def foo.identity[T](value: T) -> T: ...".to_string())
        );
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

        assert_eq!(
            print_type(&db, callable),
            Ok("() -> typing.Self".to_string())
        );
    }

    #[test]
    fn signatures_preserve_parameter_kinds_and_defaults() {
        assert_eq!(
            print_symbol(
                r#"
async def f(
    x: int,
    /,
    value: str = "default",
    *args: bytes,
    flag: bool = False,
    **kwargs: float,
) -> None:
    pass
"#,
                "f",
            ),
            Ok(concat!(
                "async def foo.f(x: builtins.int, /, value: builtins.str = ",
                "\"default\", *args: builtins.bytes, flag: builtins.bool = False, ",
                "**kwargs: builtins.float) -> None: ..."
            )
            .to_string())
        );
        assert_eq!(
            print_symbol(
                "def defaults(value, count=1) -> int:\n    return count\n",
                "defaults",
            ),
            Ok(concat!(
                "def foo.defaults(value: Unknown, count: Unknown = 1) -> ",
                "builtins.int: ..."
            )
            .to_string())
        );
    }

    #[test]
    fn anonymous_callable_and_bound_method() {
        assert_eq!(
            print_symbol(
                r#"
from collections.abc import Callable
callback: Callable[[int, str], bool]
"#,
                "callback",
            ),
            Ok("(builtins.int, builtins.str, /) -> builtins.bool".to_string())
        );
        assert_eq!(
            print_symbol(
                r#"
class C:
    def method(self, value: int) -> str:
        return str(value)

method = C().method
"#,
                "method",
            ),
            Ok("def foo.C.method(value: builtins.int) -> builtins.str: ...".to_string())
        );
        assert_eq!(
            print_symbol(
                r#"
from collections.abc import Callable
from typing import Concatenate

callback: Callable[Concatenate[int, ...], str]
"#,
                "callback",
            ),
            Ok(concat!(
                "(builtins.int, /, *args: typing.Any, **kwargs: typing.Any) -> ",
                "builtins.str"
            )
            .to_string())
        );
    }

    #[test]
    fn overloads_are_an_explicit_group() {
        assert_eq!(
            print_symbol(
                r#"
from typing import overload

@overload
def convert(value: int) -> str: ...
@overload
def convert(value: str) -> int: ...
def convert(value: int | str) -> int | str:
    return value
"#,
                "convert",
            ),
            Ok(concat!(
                "Overloads[def foo.convert(value: builtins.int) -> builtins.str: ..., ",
                "def foo.convert(value: builtins.str) -> builtins.int: ...]"
            )
            .to_string())
        );
    }

    #[test]
    fn literals_and_tuples_are_complete() {
        assert_eq!(
            print_symbol("value = (1, \"two\", b\"three\", True)\n", "value"),
            Ok(concat!(
                "builtins.tuple[typing.Literal[1], typing.Literal[\"two\"], ",
                "typing.Literal[b\"three\"], typing.Literal[True]]"
            )
            .to_string())
        );
        assert_eq!(
            print_symbol("value = \"\\u0085\"\n", "value"),
            Ok("typing.Literal[\"\\x85\"]".to_string())
        );
    }

    #[test]
    fn float_and_complex_use_exact_or_canonical_spellings() {
        assert_eq!(
            print_symbol("value = (1.0, 1j)\n", "value"),
            Ok(concat!(
                "builtins.tuple[ty_extensions.JustFloat, ",
                "ty_extensions.JustComplex]"
            )
            .to_string())
        );
        assert_eq!(
            print_symbol(
                "from typing import Literal\nvalue: float | Literal['x']\n",
                "value",
            ),
            Ok("builtins.float | typing.Literal[\"x\"]".to_string())
        );
    }

    #[test]
    fn provide_type_normalizes_public_numeric_types_recursively() {
        assert_eq!(
            print_provided_symbol("value = (1.0, 1j)\n", "value"),
            Ok("builtins.tuple[builtins.float, builtins.complex]".to_string())
        );

        let db = setup_db();
        let generic =
            KnownClass::List.to_specialized_instance(&db, &[KnownClass::Float.to_instance(&db)]);
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

        assert_eq!(
            print_type(&db, generic),
            Ok("builtins.list[ty_extensions.JustFloat]".to_string())
        );
        assert_eq!(
            print_type_for_provide_type(&db, generic),
            Ok("builtins.list[builtins.float]".to_string())
        );
        assert_eq!(
            print_type(&db, callable),
            Ok("(ty_extensions.JustFloat, /) -> ty_extensions.JustComplex".to_string())
        );
        assert_eq!(
            print_type_for_provide_type(&db, callable),
            Ok("(builtins.float, /) -> builtins.complex".to_string())
        );
    }

    #[test]
    fn provide_type_normalizes_runtime_alias_objects_without_resolving_aliases() {
        assert_eq!(
            print_provided_symbol("type Alias = int\nvalue = Alias\n", "value"),
            Ok("typing_extensions.TypeAliasType".to_string())
        );
        assert_eq!(
            print_provided_symbol("type Alias[T] = list[T]\nvalue = Alias[int]\n", "value"),
            Ok("types.GenericAlias".to_string())
        );
        assert_eq!(
            print_provided_symbol("type Alias = int\nvalue: Alias\n", "value"),
            Ok("foo.Alias".to_string())
        );
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

        for ty in [positive, negative] {
            assert_eq!(
                print_type_for_provide_type(&db, ty),
                Ok("ty_extensions.AlwaysTruthy".to_string())
            );
        }
        assert!(matches!(only_synthesized, Type::Intersection(_)));

        let nested = KnownClass::List.to_specialized_instance(&db, &[synthesized]);
        for ty in [synthesized, only_synthesized, nested] {
            assert_eq!(
                print_type_for_provide_type(&db, ty),
                Err(PrintTypeError::UnsupportedType {
                    kind: "synthesized protocol"
                })
            );
        }
    }

    #[test]
    fn intersection_and_negation_use_python_precedence() {
        let db = setup_db();
        let ty = IntersectionBuilder::new(&db)
            .add_positive(Type::AlwaysTruthy)
            .add_negative(KnownClass::Int.to_instance(&db))
            .build();

        assert_eq!(
            print_type(&db, ty),
            Ok("ty_extensions.AlwaysTruthy & ~builtins.int".to_string())
        );
    }

    #[test]
    fn ambiguous_declarations_are_rejected() {
        assert!(matches!(
            print_symbol(
                r#"
class C: ...
first = C()
class C: ...
value = first
"#,
                "value",
            ),
            Err(PrintTypeError::AmbiguousName { name }) if name == "C"
        ));
        let ancestor_result = print_symbol(
            r#"
class Outer:
    class C: ...

FirstC = Outer.C

class Outer:
    class C: ...

value = FirstC()
"#,
            "value",
        );
        assert!(
            matches!(
                ancestor_result,
                Err(PrintTypeError::AmbiguousName { ref name }) if name == "Outer"
            ),
            "{ancestor_result:?}"
        );
    }

    #[test]
    fn synthesized_and_internal_types_are_rejected() {
        let db = setup_db();
        let synthesized =
            Type::TypedDict(TypedDictType::Synthesized(SynthesizedTypedDictType::new(
                &db,
                TypedDictSchema::default(),
                SynthesizedTypedDictKind::Schema,
                TypedDictOpenness::default(),
            )));

        for (ty, kind) in [
            (synthesized, "synthesized TypedDict"),
            (
                Type::SpecialForm(SpecialFormType::Literal),
                "runtime typing object",
            ),
            (
                Type::function_like_callable(
                    &db,
                    Signature::new(Parameters::empty(), Type::unknown()),
                ),
                "non-regular anonymous callable",
            ),
            (
                KnownClass::ConstraintSet.to_instance(&db),
                "internal nominal type",
            ),
            (
                todo_type!("type expression printing test"),
                "internal dynamic type",
            ),
        ] {
            assert_eq!(
                print_type(&db, ty),
                Err(PrintTypeError::UnsupportedType { kind })
            );
        }
    }

    #[test]
    fn anonymous_recursion_is_rejected() {
        let db = setup_db();
        let ty = Type::Callable(CallableType::unknown(&db));
        let mut printer = PrintType {
            db: &db,
            normalization: Normalization::Exact,
            output: String::new(),
            active: FxHashSet::from_iter([ty]),
            binders: Vec::new(),
        };

        assert_eq!(
            printer.print(ty, Precedence::Callable),
            Err(PrintTypeError::RecursiveType)
        );
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
