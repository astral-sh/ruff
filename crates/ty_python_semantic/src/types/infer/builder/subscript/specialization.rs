//! Explicit specialization as owned transitions that yield argument inference to the local driver.

use std::borrow::Borrow;
use std::convert::Infallible;

use itertools::Itertools;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use ty_python_core::place_table;

use super::TypeInferenceBuilder;
use crate::{Db, ProgramEnvironment};
use crate::types::call::bind::CallableDescription;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::diagnostic::{INVALID_TYPE_ARGUMENTS, INVALID_TYPE_FORM, NOT_SUBSCRIPTABLE};
use crate::types::generics::GenericContext;
use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::instance::NominalVisitorKind;
use crate::types::tuple::{Tuple, TupleSpec, TupleSpecBuilder, TupleType};
use crate::types::typevar::{BindingContext, TypeVarSet};
use crate::types::{
    BoundTypeVarInstance, ClassType, KnownInstanceType, Parameters, StaticClassLiteral,
    SubclassOfInner, SubclassOfType, Type, TypeContext, TypeMapping, TypeVarBoundOrConstraints,
    TypeVarKind,
};

/// Selects the class or protocol represented by a specialized `type[C[T]]` annotation.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct ClassSubclassFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Semantic dependencies for constructing `type[C[T]]` after validating its arguments.
    #[synchronous(SynchronousClassSubclassEffects)]
    pub(in crate::types::infer) trait ClassSubclassEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn specialize(&self, class: StaticClassLiteral<'db>, generic_context: GenericContext<'db>, types: &[Option<Type<'db>>]) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn instance(&self, env: &ProgramEnvironment<'db>, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass(&self, env: &ProgramEnvironment<'db>, inner: SubclassOfInner<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ClassSubclassFacts {
        fn class<'db>(&self, class: ClassType<'db>) -> SubclassOfInner<'db> {
            SubclassOfInner::Class(class)
        }

        fn instance<'db>(&self, class: ClassType<'db>, instance: Type<'db>) -> SubclassOfInner<'db> {
            if let Type::ProtocolInstance(protocol) = instance {
                SubclassOfInner::Protocol(protocol)
            } else {
                SubclassOfInner::Class(class)
            }
        }
    }

    /// Constructs `type[C[T]]`, retaining a protocol's structural subclass representation.
    #[synchronous(class_subclass_sync)]
    #[capabilities(effects = ClassSubclassEffects, facts = ClassSubclassFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn class_subclass_with<'db, E: ClassSubclassEffects<'db>>(
        env: &ProgramEnvironment<'db>, class: StaticClassLiteral<'db>, generic_context: GenericContext<'db>, types: &[Option<Type<'db>>], facts: ClassSubclassFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        let specialized = effects.specialize(class, generic_context, types).await?;
        let inner = if effects.is_protocol(class).await? {
            let instance = effects.instance(env, specialized).await?;
            facts.instance(specialized, instance)
        } else {
            facts.class(specialized)
        };
        effects.subclass(env, inner).await
    }
}

/// Constructs the subclass result after class arguments have been checked.
pub(in crate::types::infer) fn finish_class_subclass<'db>(
    builder: &TypeInferenceBuilder<'db, '_>,
    class: StaticClassLiteral<'db>,
    generic_context: GenericContext<'db>,
    types: &[Option<Type<'db>>],
) -> Type<'db> {
    match class_subclass_sync(
        builder.program_environment(),
        class,
        generic_context,
        types,
        ClassSubclassFacts,
        &OrdinaryClassSubclassEffects { db: builder.db() },
    ) {
        Ok(ty) => ty,
        Err(never) => match never {},
    }
}

struct OrdinaryClassSubclassEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousClassSubclassEffects<'db> for OrdinaryClassSubclassEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn specialize(
        &self,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
        types: &[Option<Type<'db>>],
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.apply_specialization(self.db, |_| {
            generic_context.specialize_partial(self.db, types.iter().copied())
        }))
    }

    fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_protocol(self.db))
    }

    fn instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::instance(self.db, env, class))
    }

    fn subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        inner: SubclassOfInner<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(SubclassOfType::from(self.db, env, inner))
    }
}

pub(in crate::types::infer) struct Request<'db, 'expr, T> {
    pub(in crate::types::infer) subscript: &'expr ast::ExprSubscript,
    pub(in crate::types::infer) value_ty: Type<'db>,
    pub(in crate::types::infer) generic_context: GenericContext<'db>,
    pub(in crate::types::infer) target: T,
    pub(in crate::types::infer) protocol_guard: Option<StaticClassLiteral<'db>>,
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) enum ExplicitSpecializationError {
    InvalidParamSpec,
    ParamSpecForTypeVar,
    UnsatisfiedBound,
    UnsatisfiedConstraints,
    /// These two errors override the errors above, causing all specializations to be `Unknown`.
    MissingTypeVars,
    TooManyArguments,
    /// This error overrides the errors above, causing the type itself to be `Unknown`.
    NonGeneric,
}

/// A type argument after expanding any allowed `Unpack[tuple[...]]` syntax.
#[derive(Clone, Copy)]
pub(in crate::types::infer) struct TypeArgument<'db, 'expr> {
    /// The source expression used for diagnostics and deferred inference.
    pub(in crate::types::infer) node: &'expr ast::Expr,
    /// The already-inferred type, if this argument did not need deferred inference.
    pub(in crate::types::infer) ty: Option<Type<'db>>,
    /// The index of the original source argument before any `Unpack` expansion.
    pub(in crate::types::infer) source_index: usize,
}

pub(in crate::types::infer) struct Context<'db, 'expr, B, T> {
    request: Request<'db, 'expr, T>,
    body: Body<'db, 'expr, B>,
}

pub(in crate::types::infer) struct Body<'db, 'expr, B> {
    constraints: B,
    previously_allowed_paramspec: bool,
    previously_disabled_int_float_special_case: Option<bool>,
    exactly_one_paramspec: bool,
    type_arguments: &'expr [ast::Expr],
    store_inferred_type_arguments: bool,
    inferred_type_arguments: Vec<Option<Type<'db>>>,
    typevars: Vec<BoundTypeVarInstance<'db>>,
    typevartuple_index: Option<usize>,
    expanded_type_arguments: Vec<TypeArgument<'db, 'expr>>,
    specialization_types: Vec<Option<Type<'db>>>,
    typevar_with_defaults: usize,
    missing_typevars: Vec<BoundTypeVarInstance<'db>>,
    first_excess_type_argument_index: Option<usize>,
    error: Option<ExplicitSpecializationError>,
}

pub(in crate::types::infer) struct SavedFlags {
    previously_allowed_paramspec: bool,
    previously_disabled_int_float_special_case: Option<bool>,
}

impl<B> Body<'_, '_, B> {
    pub(in crate::types::infer) fn retire(self) -> SavedFlags {
        SavedFlags {
            previously_allowed_paramspec: self.previously_allowed_paramspec,
            previously_disabled_int_float_special_case: self
                .previously_disabled_int_float_special_case,
        }
    }
}

pub(in crate::types::infer) struct Packing<'db, 'expr> {
    typevartuple_index: usize,
    typevartuple_end: usize,
    suffix_len: usize,
    packed: Vec<TypeArgument<'db, 'expr>>,
    packed_suffix: Vec<TypeArgument<'db, 'expr>>,
    tuple_builder: TupleSpecBuilder<'db>,
}

pub(in crate::types::infer) enum State<'db, 'expr, B, T> {
    Start(Request<'db, 'expr, T>),
    Active(Context<'db, 'expr, B, T>, Phase<'db, 'expr>),
}

impl<'db, 'expr, B, T> State<'db, 'expr, B, T> {
    pub(in crate::types::infer) fn new(request: Request<'db, 'expr, T>) -> Self {
        Self::Start(request)
    }

    #[cfg(all(test, feature = "experimental-analysis"))]
    pub(in crate::types::infer) fn constraint_builder(&self) -> Option<&B> {
        let context = match self {
            Self::Start(_) => return None,
            Self::Active(context, _) => context,
        };
        Some(&context.body.constraints)
    }
}

pub(in crate::types::infer) enum Phase<'db, 'expr> {
    LocateVariadic(usize),
    Expand(usize),
    Expanded(usize, Type<'db>),
    CheckUnpack(usize, Type<'db>, &'db [Type<'db>], usize),
    AppendUnpack(usize, &'db [Type<'db>], usize),
    PackStart,
    PackPrefix(Packing<'db, 'expr>, usize),
    PackMiddle(Packing<'db, 'expr>, usize),
    PackedMiddle(Packing<'db, 'expr>, usize, Type<'db>),
    PackSuffix(Packing<'db, 'expr>, usize),
    ValidateStart,
    Validate(usize),
    ValidateProvided(usize, Type<'db>),
    Finalize,
    Recovery(usize, Vec<Option<Type<'db>>>),
}

pub(in crate::types::infer) enum ChildRequest<'expr> {
    TypeExpression(&'expr ast::Expr),
    Expression(&'expr ast::Expr),
}

enum ReturnTo<'db, 'expr> {
    Expand(usize),
    PackMiddle(Packing<'db, 'expr>, usize),
    Validate(usize),
}

pub(in crate::types::infer) struct Pending<'db, 'expr, B, T> {
    context: Context<'db, 'expr, B, T>,
    return_to: ReturnTo<'db, 'expr>,
    previously_in_valid_unpack_context: Option<bool>,
}

#[cfg(all(test, feature = "experimental-analysis"))]
impl<B, T> Pending<'_, '_, B, T> {
    pub(in crate::types::infer) fn constraint_builder(&self) -> &B {
        &self.context.body.constraints
    }
}

pub(in crate::types::infer) struct Completed<'db, 'expr, B, T> {
    context: Context<'db, 'expr, B, T>,
    result: Option<Type<'db>>,
}

#[cfg(all(test, feature = "experimental-analysis"))]
impl<B, T> Completed<'_, '_, B, T> {
    pub(in crate::types::infer) fn constraint_builder(&self) -> &B {
        &self.context.body.constraints
    }
}

pub(in crate::types::infer) enum Action<'db, 'expr, B, T> {
    Continue(State<'db, 'expr, B, T>),
    Infer {
        pending: Pending<'db, 'expr, B, T>,
        request: ChildRequest<'expr>,
    },
    Complete(Completed<'db, 'expr, B, T>),
}

pub(in crate::types::infer) enum Report<'db, 'expr, 'a> {
    InvalidUnpack(&'expr ast::Expr),
    SplitTypeVarTuple(&'expr ast::Expr),
    ParamSpecForTypeVar {
        node: &'expr ast::Expr,
        provided: BoundTypeVarInstance<'db>,
        typevar: BoundTypeVarInstance<'db>,
    },
    UnsatisfiedBound {
        node: &'expr ast::Expr,
        provided: Type<'db>,
        bound: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
    },
    UnsatisfiedConstraints {
        node: &'expr ast::Expr,
        provided: Type<'db>,
        constraints: crate::types::typevar::TypeVarConstraints<'db>,
        typevar: BoundTypeVarInstance<'db>,
    },
    Missing {
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        variables: &'a [BoundTypeVarInstance<'db>],
    },
    NonGeneric {
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
    },
    TooMany {
        node: &'expr ast::Expr,
        value_ty: Type<'db>,
        typevars_len: usize,
        typevar_with_defaults: usize,
        provided: usize,
    },
}

pub(in crate::types::infer) struct ExplicitSpecializationFacts;

pub(in crate::types::infer) trait OrdinarySpecializationFinalizer<'db> {
    type Target;
    fn finish_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        target: Self::Target,
        generic_context: GenericContext<'db>,
        types: &[Option<Type<'db>>],
    ) -> Type<'db>;
}

pub(in crate::types::infer) struct OrdinarySpecializationEffects<'db, F> {
    pub(in crate::types::infer) db: &'db dyn Db,
    pub(in crate::types::infer) finalizer: F,
}

fn add_typevar_definition<'db>(
    db: &'db dyn Db,
    diagnostic: &mut Diagnostic,
    typevar: BoundTypeVarInstance<'db>,
) {
    let Some(definition) = typevar.typevar(db).definition(db) else {
        return;
    };
    let file = definition.file(db);
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let range = definition.focus_range(db, &module).range();
    diagnostic.annotate(
        Annotation::secondary(Span::from(file).with_range(range))
            .message("Type variable defined here"),
    );
}

fn report_ordinary<'db>(inference: &TypeInferenceBuilder<'db, '_>, report: Report<'db, '_, '_>) {
    let db = inference.db();
    let env = inference.program_environment();
    match report {
        Report::InvalidUnpack(node) => {
            if let Some(builder) = inference.context.report_lint(&INVALID_TYPE_FORM, node) {
                builder.into_diagnostic(
                    "`Unpack` can only be used with a fixed tuple type in this context",
                );
            }
        }
        Report::SplitTypeVarTuple(node) => {
            if let Some(builder) = inference.context.report_lint(&INVALID_TYPE_FORM, node) {
                builder.into_diagnostic(
                    "A TypeVarTuple cannot be split to provide a fixed type argument",
                );
            }
        }
        Report::ParamSpecForTypeVar {
            node,
            provided: tv,
            typevar,
        } => {
            if let Some(builder) = inference.context.report_lint(&INVALID_TYPE_ARGUMENTS, node) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "ParamSpec `{}` cannot be used to specialize \
                        type variable `{}`",
                    tv.typevar(db).name(db),
                    typevar.name(db),
                ));
                for (kind, var) in [("ParamSpec", tv), ("Type variable", typevar)] {
                    let Some(definition) = var.typevar(db).definition(db) else {
                        continue;
                    };
                    let file = definition.file(db);
                    let module = parsed_module(db, definition.python_file(db)).load(db);
                    let range = definition.focus_range(db, &module).range();
                    diagnostic.annotate(
                        Annotation::secondary(Span::from(file).with_range(range))
                            .message(format_args!("{kind} `{}` defined here", var.name(db))),
                    );
                }
            }
        }
        Report::UnsatisfiedBound {
            node,
            provided: type_to_check,
            bound,
            typevar,
        } => {
            if let Some(builder) = inference.context.report_lint(&INVALID_TYPE_ARGUMENTS, node) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Type `{}` is not assignable to upper bound `{}` \
                        of type variable `{}`",
                    type_to_check.display(db, env),
                    bound.display(db, env),
                    typevar.identity(db).display(db),
                ));
                add_typevar_definition(db, &mut diagnostic, typevar);
                type_to_check
                    .assignability_error_context(db, env, bound)
                    .attach_to(db, env, &mut diagnostic);
            }
        }
        Report::UnsatisfiedConstraints {
            node,
            provided: type_to_check,
            constraints: typevar_constraints,
            typevar,
        } => {
            if let Some(builder) = inference.context.report_lint(&INVALID_TYPE_ARGUMENTS, node) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Type `{}` does not satisfy constraints `{}` \
                        of type variable `{}`",
                    type_to_check.display(db, env),
                    typevar_constraints
                        .elements(db)
                        .iter()
                        .map(|c| c.display(db, env))
                        .format("`, `"),
                    typevar.identity(db).display(db),
                ));
                add_typevar_definition(db, &mut diagnostic, typevar);
            }
        }
        Report::Missing {
            subscript,
            value_ty,
            variables: missing_typevars,
        } => {
            if let Some(builder) = inference
                .context
                .report_lint(&INVALID_TYPE_ARGUMENTS, subscript)
            {
                let description = CallableDescription::new(db, value_ty);
                let s = if missing_typevars.len() > 1 { "s" } else { "" };
                builder.into_diagnostic(format_args!(
                    "No type argument{s} provided for required type variable{s} `{}`{}",
                    missing_typevars
                        .iter()
                        .map(|tv| tv.typevar(db).name(db))
                        .format("`, `"),
                    description
                        .map(|description| format!(" of {description}"))
                        .unwrap_or_default(),
                ));
            }
        }
        Report::NonGeneric {
            subscript,
            value_ty,
        } => {
            if let Some(builder) = inference.context.report_lint(&NOT_SUBSCRIPTABLE, subscript) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Cannot subscript non-generic type `{}`",
                    value_ty.display(db, env)
                ));
                let already_specialized = match value_ty {
                    Type::GenericAlias(_) => true,
                    Type::KnownInstance(KnownInstanceType::UnionType(union)) => union
                        .value_expression_types(db, env)
                        .is_ok_and(|mut tys| tys.any(|ty| ty.is_generic_alias())),
                    _ => false,
                };
                if already_specialized {
                    diagnostic.annotate(
                        inference
                            .context
                            .secondary(&*subscript.value)
                            .message("Type is already specialized"),
                    );
                }
            }
        }
        Report::TooMany {
            node,
            value_ty,
            typevars_len,
            typevar_with_defaults,
            provided,
        } => {
            if let Some(builder) = inference.context.report_lint(&INVALID_TYPE_ARGUMENTS, node) {
                let description = CallableDescription::new(db, value_ty);
                builder.into_diagnostic(format_args!(
                    "Too many type arguments{}: expected {}, got {}",
                    description
                        .map(|description| format!(" to {description}"))
                        .unwrap_or_default(),
                    if typevar_with_defaults == 0 {
                        format!("{typevars_len}")
                    } else {
                        format!(
                            "between {} and {}",
                            typevars_len - typevar_with_defaults,
                            typevars_len
                        )
                    },
                    provided,
                ));
            }
        }
    }
}

impl<'db, 'ast, F: OrdinarySpecializationFinalizer<'db>>
    SynchronousExplicitSpecializationEffects<'db, 'ast> for OrdinarySpecializationEffects<'db, F>
{
    type Error = Infallible;
    type Builder = ConstraintSetBuilder<'db>;
    type Target = F::Target;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.is_protocol(self.db))
    }
    fn declares_class_member(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(place_table(self.db, class.body_scope(self.db))
            .symbol_id("__class__")
            .is_some())
    }
    fn protocol_writable_member(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> Result<bool, Infallible> {
        Ok(class
            .identity_specialization(self.db)
            .into_protocol_class(self.db)
            .is_some_and(|protocol| {
                protocol
                    .interface(self.db)
                    .includes_generic_writable_instance_member(
                        self.db,
                        builder.program_environment(),
                        "__class__",
                        context,
                    )
            }))
    }
    fn replace_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        flag: InferenceFlags,
        value: bool,
    ) -> Result<bool, Infallible> {
        Ok(builder.context.inference_flags.replace(flag, value))
    }
    fn restore_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        flag: InferenceFlags,
        value: bool,
    ) -> Result<(), Infallible> {
        builder.context.inference_flags.set(flag, value);
        Ok(())
    }
    fn new_builder(&self) -> Result<Self::Builder, Infallible> {
        Ok(ConstraintSetBuilder::new())
    }
    fn exactly_one_paramspec(&self, context: GenericContext<'db>) -> Result<bool, Infallible> {
        Ok(context.exactly_one_paramspec(self.db))
    }
    fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<Vec<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(context.variables(self.db).collect())
    }
    fn variable_kind(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarKind, Infallible> {
        Ok(variable.typevar(self.db).kind(self.db))
    }
    fn new_inferred(&self, len: usize) -> Result<Vec<Option<Type<'db>>>, Infallible> {
        Ok(vec![None; len])
    }
    fn new_arguments<'expr>(
        &self,
        capacity: usize,
    ) -> Result<Vec<TypeArgument<'db, 'expr>>, Infallible> {
        Ok(Vec::with_capacity(capacity))
    }
    fn new_types(&self, capacity: usize) -> Result<Vec<Option<Type<'db>>>, Infallible> {
        Ok(Vec::with_capacity(capacity))
    }
    fn push_argument<'expr>(
        &self,
        arguments: &mut Vec<TypeArgument<'db, 'expr>>,
        argument: TypeArgument<'db, 'expr>,
    ) -> Result<(), Infallible> {
        arguments.push(argument);
        Ok(())
    }
    fn extend_arguments<'expr>(
        &self,
        arguments: &mut Vec<TypeArgument<'db, 'expr>>,
        suffix: Vec<TypeArgument<'db, 'expr>>,
    ) -> Result<(), Infallible> {
        arguments.extend(suffix);
        Ok(())
    }
    fn push_type(
        &self,
        types: &mut Vec<Option<Type<'db>>>,
        ty: Option<Type<'db>>,
    ) -> Result<(), Infallible> {
        types.push(ty);
        Ok(())
    }
    fn push_missing(
        &self,
        variables: &mut Vec<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        variables.push(variable);
        Ok(())
    }
    fn expression_flags(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<TypeExpressionFlags, Infallible> {
        Ok(builder.type_expression_flags(expression))
    }
    fn exact_tuple(&self, ty: Type<'db>) -> Result<Option<&'db TupleSpec<'db>>, Infallible> {
        Ok(
            match ty
                .as_nominal_instance()
                .map(|instance| instance.visitor_kind())
            {
                Some(NominalVisitorKind::Tuple(tuple)) => Some(tuple.tuple(self.db)),
                _ => None,
            },
        )
    }
    fn new_tuple_builder(&self, capacity: usize) -> Result<TupleSpecBuilder<'db>, Infallible> {
        Ok(TupleSpecBuilder::with_capacity(capacity))
    }
    fn tuple_concat(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: &mut TupleSpecBuilder<'db>,
        other: &TupleSpec<'db>,
    ) -> Result<(), Infallible> {
        *tuple = std::mem::replace(tuple, TupleSpecBuilder::with_capacity(0)).concat(
            self.db,
            builder.program_environment(),
            other,
        );
        Ok(())
    }
    fn tuple_concat_typevar(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: &mut TupleSpecBuilder<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        *tuple = std::mem::replace(tuple, TupleSpecBuilder::with_capacity(0))
            .concat_variadic_typevar(self.db, builder.program_environment(), variable);
        Ok(())
    }
    fn tuple_push(
        &self,
        tuple: &mut TupleSpecBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        tuple.push(ty);
        Ok(())
    }
    fn finish_tuple(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: TupleSpecBuilder<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::tuple(TupleType::new(
            self.db,
            builder.program_environment(),
            &tuple.build(),
        )))
    }
    fn pack_fixed<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        packing: &mut Packing<'db, 'expr>,
        argument: TypeArgument<'db, 'expr>,
        suffix: bool,
    ) -> Result<(), Infallible> {
        pack_fixed_sync(
            builder,
            packing,
            argument,
            suffix,
            ExplicitSpecializationFacts,
            self,
        )
    }
    fn paramspec(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        exactly_one: bool,
    ) -> Result<Result<Type<'db>, ()>, Infallible> {
        Ok(builder.infer_paramspec_explicit_specialization_value(expression, exactly_one))
    }
    fn unknown_paramspec(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::paramspec_value_callable(
            self.db,
            Parameters::unknown(),
        ))
    }
    fn unknown_variadic(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::homogeneous_tuple(
            self.db,
            builder.program_environment(),
            Type::unknown(),
        ))
    }
    fn default_type(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(variable.default_type(self.db))
    }
    fn bound_or_constraints(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Infallible> {
        Ok(variable
            .typevar(self.db)
            .bound_or_constraints(self.db, builder.program_environment()))
    }
    fn bind_validation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        let env = builder.program_environment();
        Ok(ty.apply_type_mapping(
            self.db,
            env,
            &TypeMapping::BindLegacyTypevars(BindingContext::Synthetic(env.program(self.db))),
            TypeContext::default(),
        ))
    }
    fn constraints_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraints: crate::types::typevar::TypeVarConstraints<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(constraints.as_type(self.db, builder.program_environment()))
    }
    fn never_assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraints: &ConstraintSetBuilder<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        let env = builder.program_environment();
        Ok(source
            .when_assignable_to(self.db, env, target, constraints, TypeVarSet::None)
            .is_never_satisfied(self.db, env))
    }
    fn report(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        report: Report<'db, '_, '_>,
    ) -> Result<(), Infallible> {
        report_ordinary(builder, report);
        Ok(())
    }
    fn store_inferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        types: Vec<Option<Type<'db>>>,
    ) -> Result<(), Infallible> {
        builder.store_expression_type(
            slice,
            Type::heterogeneous_tuple(
                self.db,
                builder.program_environment(),
                types.into_iter().map(|ty| ty.unwrap_or(Type::unknown())),
            ),
        );
        Ok(())
    }
    fn finish_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: Self::Target,
        generic_context: GenericContext<'db>,
        types: &[Option<Type<'db>>],
    ) -> Result<Type<'db>, Infallible> {
        Ok(self
            .finalizer
            .finish_target(builder, target, generic_context, types))
    }
    fn retire_body<'expr>(
        &self,
        body: Body<'db, 'expr, Self::Builder>,
    ) -> Result<SavedFlags, Infallible> {
        Ok(body.retire())
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousExplicitSpecializationEffects)]
    pub(in crate::types::infer) trait ExplicitSpecializationEffects<'db, 'ast> {
        type Error;
        type Builder: Borrow<ConstraintSetBuilder<'db>>;
        type Target;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn declares_class_member(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn protocol_writable_member(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: StaticClassLiteral<'db>, context: GenericContext<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn replace_flag(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, flag: InferenceFlags, value: bool) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn restore_flag(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, flag: InferenceFlags, value: bool) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_builder(&self) -> Result<Self::Builder, Self::Error>;
        #[operation(source)]
        async fn exactly_one_paramspec(&self, context: GenericContext<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<Vec<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(local)]
        async fn new_inferred(&self, len: usize) -> Result<Vec<Option<Type<'db>>>, Self::Error>;
        #[operation(local)]
        async fn new_arguments<'expr>(&self, capacity: usize) -> Result<Vec<TypeArgument<'db, 'expr>>, Self::Error>;
        #[operation(local)]
        async fn new_types(&self, capacity: usize) -> Result<Vec<Option<Type<'db>>>, Self::Error>;
        #[operation(local)]
        async fn push_argument<'expr>(&self, arguments: &mut Vec<TypeArgument<'db, 'expr>>, argument: TypeArgument<'db, 'expr>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn extend_arguments<'expr>(&self, arguments: &mut Vec<TypeArgument<'db, 'expr>>, suffix: Vec<TypeArgument<'db, 'expr>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_type(&self, types: &mut Vec<Option<Type<'db>>>, ty: Option<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push_missing(&self, variables: &mut Vec<BoundTypeVarInstance<'db>>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn expression_flags(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<TypeExpressionFlags, Self::Error>;
        #[operation(source)]
        async fn exact_tuple(&self, ty: Type<'db>) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_tuple_builder(&self, capacity: usize) -> Result<TupleSpecBuilder<'db>, Self::Error>;
        #[operation(child)]
        async fn tuple_concat(&self, builder: &TypeInferenceBuilder<'db, 'ast>, tuple: &mut TupleSpecBuilder<'db>, other: &TupleSpec<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn tuple_concat_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, tuple: &mut TupleSpecBuilder<'db>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn tuple_push(&self, tuple: &mut TupleSpecBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish_tuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, tuple: TupleSpecBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn pack_fixed<'expr>(&self, builder: &TypeInferenceBuilder<'db, 'ast>, packing: &mut Packing<'db, 'expr>, argument: TypeArgument<'db, 'expr>, suffix: bool) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn paramspec(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, exactly_one: bool) -> Result<Result<Type<'db>, ()>, Self::Error>;
        #[operation(child)]
        async fn unknown_paramspec(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn unknown_variadic(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn default_type(&self, variable: BoundTypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn bound_or_constraints(&self, builder: &TypeInferenceBuilder<'db, 'ast>, variable: BoundTypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
        #[operation(child)]
        async fn bind_validation(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn constraints_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, constraints: crate::types::typevar::TypeVarConstraints<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn never_assignable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, constraints: &ConstraintSetBuilder<'db>, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn report(&self, builder: &TypeInferenceBuilder<'db, 'ast>, report: Report<'db, '_, '_>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn store_inferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, types: Vec<Option<Type<'db>>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish_target(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: Self::Target, generic_context: GenericContext<'db>, types: &[Option<Type<'db>>]) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn retire_body<'expr>(&self, body: Body<'db, 'expr, Self::Builder>) -> Result<SavedFlags, Self::Error>;
    }

    #[finite_capability]
    impl ExplicitSpecializationFacts {
        fn slice<'expr>(&self, subscript: &'expr ast::ExprSubscript) -> &'expr ast::Expr { &subscript.slice }
        fn arguments<'expr>(&self, slice: &'expr ast::Expr, exactly_one_paramspec: bool) -> (&'expr [ast::Expr], bool) {
            match slice {
                ast::Expr::Tuple(tuple) if !(exactly_one_paramspec && !tuple.elts.is_empty()) => (&tuple.elts, true),
                _ => (std::slice::from_ref(slice), false),
            }
        }
        fn argument_count(&self, arguments: &[ast::Expr]) -> usize { arguments.len() }
        fn empty_types<'db>(&self) -> Vec<Option<Type<'db>>> { Vec::new() }
        fn empty_arguments<'db, 'expr>(&self) -> Vec<TypeArgument<'db, 'expr>> { Vec::new() }
        fn empty_variables<'db>(&self) -> Vec<BoundTypeVarInstance<'db>> { Vec::new() }
        fn next(&self, index: usize) -> usize { index + 1 }
        fn paramspec(&self, kind: TypeVarKind) -> bool { kind.is_paramspec() }
        fn variadic(&self, kind: TypeVarKind) -> bool { kind.is_typevartuple() }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn typevar<'db>(&self, ty: Type<'db>) -> Option<BoundTypeVarInstance<'db>> { ty.as_typevar() }
        fn variable<'db, B>(&self, body: &Body<'db, '_, B>, index: usize) -> Option<BoundTypeVarInstance<'db>> { body.typevars.get(index).copied() }
        fn variable_count<B>(&self, body: &Body<'_, '_, B>) -> usize { body.typevars.len() }
        fn set_variadic<B>(&self, body: &mut Body<'_, '_, B>, index: usize) { body.typevartuple_index = Some(index); }
        fn source<'expr, B>(&self, body: &Body<'_, 'expr, B>, index: usize) -> Option<&'expr ast::Expr> { body.type_arguments.get(index) }
        fn source_variable<'db, B>(&self, body: &Body<'db, '_, B>, source_index: usize) -> Option<BoundTypeVarInstance<'db>> {
            if let Some(typevartuple_index) = body.typevartuple_index {
                let suffix_len = body.typevars.len() - typevartuple_index - 1;
                let suffix_source_start = body.type_arguments.len().saturating_sub(suffix_len);
                if suffix_len > 0
                    && body.type_arguments.len() >= typevartuple_index + suffix_len
                    && source_index >= suffix_source_start
                {
                    let suffix_index = source_index - suffix_source_start;
                    return body.typevars.get(body.typevars.len() - suffix_len + suffix_index).copied();
                }
            }
            body.typevars.get(body.expanded_type_arguments.len()).copied()
        }
        fn non_generic<B>(&self, body: &Body<'_, '_, B>) -> bool { body.typevars.is_empty() }
        fn inferred<'db, B>(&self, body: &mut Body<'db, '_, B>, index: usize, ty: Type<'db>) { body.inferred_type_arguments[index] = Some(ty); }
        fn argument<'db, 'expr, B>(&self, body: &Body<'db, 'expr, B>, index: usize) -> Option<TypeArgument<'db, 'expr>> { body.expanded_type_arguments.get(index).copied() }
        fn unpack(&self, flags: TypeExpressionFlags) -> bool { flags.contains(TypeExpressionFlags::UNPACK) }
        fn invalid_unpack<B>(&self, builder: &TypeInferenceBuilder<'_, '_>, body: &Body<'_, '_, B>, flags: TypeExpressionFlags) -> bool {
            flags.contains(TypeExpressionFlags::UNPACK)
                && !builder.inference_flags().contains(InferenceFlags::IN_KWARG_ANNOTATION)
                && body.typevartuple_index.is_none()
                && !flags.contains(TypeExpressionFlags::INVALID_UNPACK)
        }
        fn fixed<'db>(&self, tuple: Option<&'db TupleSpec<'db>>) -> Option<&'db [Type<'db>]> {
            match tuple { Some(Tuple::Fixed(tuple)) => Some(tuple.elements_slice()), _ => None }
        }
        fn can_expand<B>(&self, body: &Body<'_, '_, B>) -> bool { body.expanded_type_arguments.len() <= body.typevars.len() }
        fn unpack_variable<'db, B>(&self, body: &Body<'db, '_, B>, tuple: &[Type<'db>], index: usize) -> Option<BoundTypeVarInstance<'db>> {
            if index < tuple.len() { body.typevars.get(body.expanded_type_arguments.len() + index).copied() } else { None }
        }
        fn element<'db>(&self, tuple: &[Type<'db>], index: usize) -> Option<Type<'db>> { tuple.get(index).copied() }
        fn source_argument<'db, 'expr, B>(&self, body: &Body<'db, 'expr, B>, source_index: usize, ty: Option<Type<'db>>) -> TypeArgument<'db, 'expr> {
            TypeArgument { node: &body.type_arguments[source_index], ty, source_index }
        }
        fn packing_shape<B>(&self, body: &Body<'_, '_, B>, index: usize) -> (usize, usize, usize) {
            let suffix_len = body.typevars.len() - index - 1;
            let end = body.expanded_type_arguments.len().saturating_sub(suffix_len).max(index);
            (suffix_len, end, end.saturating_sub(index))
        }
        fn prefix_argument<'db, 'expr, B>(&self, body: &Body<'db, 'expr, B>, packing: &Packing<'db, 'expr>, index: usize) -> Option<TypeArgument<'db, 'expr>> {
            if index < packing.typevartuple_index { body.expanded_type_arguments.get(index).copied() } else { None }
        }
        fn middle_start<B>(&self, body: &Body<'_, '_, B>, packing: &Packing<'_, '_>) -> usize { body.expanded_type_arguments.len().min(packing.typevartuple_index) }
        fn middle_argument<'db, 'expr, B>(&self, body: &Body<'db, 'expr, B>, packing: &Packing<'db, 'expr>, index: usize) -> Option<TypeArgument<'db, 'expr>> {
            if index < packing.typevartuple_end { body.expanded_type_arguments.get(index).copied() } else { None }
        }
        fn suffix_argument<'db, 'expr, B>(&self, body: &Body<'db, 'expr, B>, packing: &Packing<'db, 'expr>, index: usize) -> Option<TypeArgument<'db, 'expr>> {
            if packing.suffix_len > 0 && body.expanded_type_arguments.len() >= packing.typevartuple_index + packing.suffix_len && index < packing.suffix_len {
                body.expanded_type_arguments.get(body.expanded_type_arguments.len() - packing.suffix_len + index).copied()
            } else { None }
        }
        fn homogeneous<'db>(&self, tuple: Option<&TupleSpec<'db>>) -> Option<Type<'db>> {
            match tuple {
                Some(Tuple::Variable(variable)) if variable.prefix_elements().is_empty() && variable.suffix_elements().is_empty() => variable.variable().homogeneous_type(),
                _ => None,
            }
        }
        fn tuple_typevartuple(&self, tuple: Option<&TupleSpec<'_>>) -> bool {
            matches!(tuple, Some(Tuple::Variable(variable)) if variable.variable().typevartuple().is_some())
        }
        fn provided<'db, 'expr>(&self, argument: TypeArgument<'db, 'expr>) -> Type<'db> { argument.ty.unwrap_or_else(Type::unknown) }
        fn with_type<'db, 'expr>(&self, argument: TypeArgument<'db, 'expr>, ty: Type<'db>) -> TypeArgument<'db, 'expr> { TypeArgument { ty: Some(ty), ..argument } }
        fn packing_output<'a, 'db, 'expr>(&self, packing: &'a mut Packing<'db, 'expr>, suffix: bool) -> &'a mut Vec<TypeArgument<'db, 'expr>> {
            if suffix { &mut packing.packed_suffix } else { &mut packing.packed }
        }
        fn needs_packed_tuple<B>(&self, body: &Body<'_, '_, B>, index: usize) -> bool { body.expanded_type_arguments.len() >= index }
        fn packed_tuple_argument<'db, 'expr, B, T>(&self, context: &Context<'db, 'expr, B, T>, index: usize, ty: Type<'db>) -> TypeArgument<'db, 'expr> {
            TypeArgument {
                node: context.body.expanded_type_arguments.get(index).map_or(&context.request.subscript.slice, |argument| argument.node),
                ty: Some(ty),
                source_index: context.body.expanded_type_arguments.get(index).map_or(0, |argument| argument.source_index),
            }
        }
        fn install_packed<'db, 'expr, B>(&self, body: &mut Body<'db, 'expr, B>, packed: Vec<TypeArgument<'db, 'expr>>) { body.expanded_type_arguments = packed; }
        fn install_types<'db, B>(&self, body: &mut Body<'db, '_, B>, types: Vec<Option<Type<'db>>>) { body.specialization_types = types; }
        fn install_variables<'db, B>(&self, body: &mut Body<'db, '_, B>, variables: Vec<BoundTypeVarInstance<'db>>) { body.typevars = variables; }
        fn install_suffix<'db, 'expr>(&self, packing: &mut Packing<'db, 'expr>, suffix: Vec<TypeArgument<'db, 'expr>>) { packing.packed_suffix = suffix; }
        fn has_default<'db, B>(&self, body: &mut Body<'db, '_, B>, default: Option<Type<'db>>) -> bool {
            if default.is_some() { body.typevar_with_defaults += 1; true } else { false }
        }
        fn set_error<B>(&self, body: &mut Body<'_, '_, B>, error: ExplicitSpecializationError) { body.error = Some(error); }
        fn excess<B>(&self, body: &mut Body<'_, '_, B>, index: usize) { body.first_excess_type_argument_index.get_or_insert(index); }
        fn has_missing<B>(&self, body: &Body<'_, '_, B>) -> bool { !body.missing_typevars.is_empty() }
        fn expanded_count<B>(&self, body: &Body<'_, '_, B>) -> usize { body.expanded_type_arguments.len() }
        fn constraints<'a, 'db, B: Borrow<ConstraintSetBuilder<'db>>>(&self, body: &'a Body<'db, '_, B>) -> &'a ConstraintSetBuilder<'db> { body.constraints.borrow() }
        fn take_inferred<'db, B>(&self, body: &mut Body<'db, '_, B>) -> Vec<Option<Type<'db>>> { std::mem::take(&mut body.inferred_type_arguments) }
        fn needs_recovery<B>(&self, body: &Body<'_, '_, B>) -> bool { matches!(body.error, Some(ExplicitSpecializationError::MissingTypeVars | ExplicitSpecializationError::TooManyArguments)) }
        fn is_non_generic_error<B>(&self, body: &Body<'_, '_, B>) -> bool { matches!(body.error, Some(ExplicitSpecializationError::NonGeneric)) }
    }

    #[synchronous(pack_fixed_sync)]
    #[capabilities(effects = ExplicitSpecializationEffects, facts = ExplicitSpecializationFacts)]
    #[passive_values(Report::SplitTypeVarTuple)]
    pub(in crate::types::infer) async fn pack_fixed_with<'db, 'ast, 'expr, E: ExplicitSpecializationEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>, packing: &mut Packing<'db, 'expr>, argument: TypeArgument<'db, 'expr>, suffix: bool, facts: ExplicitSpecializationFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let provided_type = facts.provided(argument);
        let flags = effects.expression_flags(builder, argument.node).await?;
        if facts.unpack(flags) {
            let tuple = effects.exact_tuple(provided_type).await?;
            if let Some(variable_type) = facts.homogeneous(tuple)
                && let Some(tuple) = tuple
            {
                effects.tuple_concat(builder, &mut packing.tuple_builder, tuple).await?;
                return effects.push_argument(facts.packing_output(packing, suffix), facts.with_type(argument, variable_type)).await;
            }
            let split = if let Some(typevar) = facts.typevar(provided_type)
                && facts.variadic(effects.variable_kind(typevar).await?)
            {
                true
            } else {
                facts.tuple_typevartuple(effects.exact_tuple(provided_type).await?)
            };
            if split {
                effects.report(builder, Report::SplitTypeVarTuple(argument.node)).await?;
                return effects.push_argument(facts.packing_output(packing, suffix), facts.with_type(argument, facts.unknown())).await;
            }
        }
        effects.push_argument(facts.packing_output(packing, suffix), argument).await
    }

    #[synchronous(advance_sync)]
    #[capabilities(effects = ExplicitSpecializationEffects, facts = ExplicitSpecializationFacts)]
    #[passive_values(
        Context, Body, Packing, Pending, Completed, Action::Continue, Action::Infer, Action::Complete,
        State::Active, Phase::LocateVariadic, Phase::Expand, Phase::Expanded, Phase::CheckUnpack, Phase::AppendUnpack,
        Phase::PackStart, Phase::PackPrefix, Phase::PackMiddle, Phase::PackedMiddle, Phase::PackSuffix,
        Phase::ValidateStart, Phase::Validate, Phase::ValidateProvided, Phase::Finalize, Phase::Recovery,
        ReturnTo::Expand, ReturnTo::PackMiddle, ReturnTo::Validate,
        ChildRequest::Expression, ChildRequest::TypeExpression,
        InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE, InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, InferenceFlags::IN_VALID_UNPACK_CONTEXT,
        ExplicitSpecializationError::InvalidParamSpec, ExplicitSpecializationError::ParamSpecForTypeVar,
        ExplicitSpecializationError::UnsatisfiedBound, ExplicitSpecializationError::UnsatisfiedConstraints,
        ExplicitSpecializationError::MissingTypeVars, ExplicitSpecializationError::TooManyArguments, ExplicitSpecializationError::NonGeneric,
        Report::InvalidUnpack, Report::ParamSpecForTypeVar, Report::UnsatisfiedBound, Report::UnsatisfiedConstraints,
        Report::Missing, Report::NonGeneric, Report::TooMany
    )]
    pub(in crate::types::infer) async fn advance_with<'db, 'ast, 'expr, E: ExplicitSpecializationEffects<'db, 'ast>>(
        state: State<'db, 'expr, E::Builder, E::Target>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: ExplicitSpecializationFacts, effects: &E,
    ) -> Result<Action<'db, 'expr, E::Builder, E::Target>, E::Error> {
        effects.checkpoint().await?;
        let (mut context, phase) = match state {
            State::Start(request) => {
                // Avoid constructing an identity specialization and a full protocol interface for the
                // many generic protocols that do not directly declare `__class__`.
                let previously_disabled_int_float_special_case = if let Some(class) = request.protocol_guard
                    && effects.is_protocol(class).await?
                    && effects.declares_class_member(class).await?
                    && effects.protocol_writable_member(builder, class, request.generic_context).await?
                {
                    Some(effects.replace_flag(builder, InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE, true).await?)
                } else { None };
                let previously_allowed_paramspec = effects.replace_flag(builder, InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true).await?;
                let constraints = effects.new_builder().await?;
                let exactly_one_paramspec = effects.exactly_one_paramspec(request.generic_context).await?;
                let (type_arguments, store_inferred_type_arguments) = facts.arguments(facts.slice(request.subscript), exactly_one_paramspec);
                let inferred_type_arguments = effects.new_inferred(facts.argument_count(type_arguments)).await?;
                let typevars = effects.variables(request.generic_context).await?;
                let context = Context {
                    request,
                    body: Body {
                        constraints, previously_allowed_paramspec, previously_disabled_int_float_special_case,
                        exactly_one_paramspec, type_arguments, store_inferred_type_arguments, inferred_type_arguments,
                        typevars, typevartuple_index: None,
                        expanded_type_arguments: facts.empty_arguments(),
                        specialization_types: facts.empty_types(), typevar_with_defaults: 0,
                        missing_typevars: facts.empty_variables(), first_excess_type_argument_index: None, error: None,
                    },
                };
                return Ok(Action::Continue(State::Active(context, Phase::LocateVariadic(0))));
            }
            State::Active(context, phase) => (context, phase),
        };
        match phase {
            Phase::LocateVariadic(index) => {
                if let Some(typevar) = facts.variable(&context.body, index) {
                    if facts.variadic(effects.variable_kind(typevar).await?) {
                        facts.set_variadic(&mut context.body, index);
                    } else {
                        return Ok(Action::Continue(State::Active(context, Phase::LocateVariadic(facts.next(index)))));
                    }
                }
                let expanded = effects.new_arguments(facts.argument_count(context.body.type_arguments)).await?;
                facts.install_packed(&mut context.body, expanded);
                Ok(Action::Continue(State::Active(context, Phase::Expand(0))))
            }
            Phase::Expand(source_index) => {
                let Some(expr) = facts.source(&context.body, source_index) else {
                    return Ok(Action::Continue(State::Active(context, Phase::PackStart)));
                };
                let typevar = facts.source_variable(&context.body, source_index);
                let defer = if context.body.exactly_one_paramspec { true }
                    else if let Some(typevar) = typevar { facts.paramspec(effects.variable_kind(typevar).await?) }
                    else { false };
                if defer {
                    let argument = facts.source_argument(&context.body, source_index, None);
                    effects.push_argument(&mut context.body.expanded_type_arguments, argument).await?;
                    return Ok(Action::Continue(State::Active(context, Phase::Expand(facts.next(source_index)))));
                }
                let (request, previously_in_valid_unpack_context) = if facts.non_generic(&context.body) {
                    // If there are no typevars at all, this is not a generic type,
                    // so we should not infer excess arguments as type expressions.
                    // For example, `list[int][0]` — the `0` is not a type expression.
                    (ChildRequest::Expression(expr), None)
                } else {
                    (ChildRequest::TypeExpression(expr), Some(effects.replace_flag(builder, InferenceFlags::IN_VALID_UNPACK_CONTEXT, true).await?))
                };
                Ok(Action::Infer { pending: Pending { context, return_to: ReturnTo::Expand(source_index), previously_in_valid_unpack_context }, request })
            }
            Phase::Expanded(source_index, provided_type) => {
                facts.inferred(&mut context.body, source_index, provided_type);
                let argument = facts.source_argument(&context.body, source_index, Some(provided_type));
                let flags = effects.expression_flags(builder, argument.node).await?;
                if facts.unpack(flags)
                    && let Some(tuple) = facts.fixed(effects.exact_tuple(provided_type).await?)
                    && facts.can_expand(&context.body)
                {
                    return Ok(Action::Continue(State::Active(context, Phase::CheckUnpack(source_index, provided_type, tuple, 0))));
                }
                if facts.invalid_unpack(builder, &context.body, flags) {
                    effects.report(builder, Report::InvalidUnpack(argument.node)).await?;
                }
                effects.push_argument(&mut context.body.expanded_type_arguments, argument).await?;
                Ok(Action::Continue(State::Active(context, Phase::Expand(facts.next(source_index)))))
            }
            Phase::CheckUnpack(source_index, provided_type, tuple, index) => {
                if let Some(typevar) = facts.unpack_variable(&context.body, tuple, index) {
                    if !facts.paramspec(effects.variable_kind(typevar).await?) {
                        return Ok(Action::Continue(State::Active(context, Phase::CheckUnpack(source_index, provided_type, tuple, facts.next(index)))));
                    }
                    let argument = facts.source_argument(&context.body, source_index, Some(provided_type));
                    let flags = effects.expression_flags(builder, argument.node).await?;
                    if facts.invalid_unpack(builder, &context.body, flags) {
                        effects.report(builder, Report::InvalidUnpack(argument.node)).await?;
                    }
                    effects.push_argument(&mut context.body.expanded_type_arguments, argument).await?;
                    return Ok(Action::Continue(State::Active(context, Phase::Expand(facts.next(source_index)))));
                }
                // Expand `Foo[Unpack[tuple[int, str]]]` to `Foo[int, str]`. ParamSpec arguments
                // must still use their dedicated inference path.
                Ok(Action::Continue(State::Active(context, Phase::AppendUnpack(source_index, tuple, 0))))
            }
            Phase::AppendUnpack(source_index, tuple, index) => {
                if let Some(ty) = facts.element(tuple, index) {
                    let argument = facts.source_argument(&context.body, source_index, Some(ty));
                    effects.push_argument(&mut context.body.expanded_type_arguments, argument).await?;
                    Ok(Action::Continue(State::Active(context, Phase::AppendUnpack(source_index, tuple, facts.next(index)))))
                } else {
                    Ok(Action::Continue(State::Active(context, Phase::Expand(facts.next(source_index)))))
                }
            }
            Phase::PackStart => {
                if let Some(typevartuple_index) = context.body.typevartuple_index {
                    let (suffix_len, typevartuple_end, tuple_capacity) = facts.packing_shape(&context.body, typevartuple_index);
                    let packed = effects.new_arguments(facts.variable_count(&context.body)).await?;
                    let tuple_builder = effects.new_tuple_builder(tuple_capacity).await?;
                    let packing = Packing { typevartuple_index, typevartuple_end, suffix_len, packed, packed_suffix: facts.empty_arguments(), tuple_builder };
                    Ok(Action::Continue(State::Active(context, Phase::PackPrefix(packing, 0))))
                } else {
                    Ok(Action::Continue(State::Active(context, Phase::ValidateStart)))
                }
            }
            Phase::PackPrefix(mut packing, index) => {
                if let Some(argument) = facts.prefix_argument(&context.body, &packing, index) {
                    effects.pack_fixed(builder, &mut packing, argument, false).await?;
                    Ok(Action::Continue(State::Active(context, Phase::PackPrefix(packing, facts.next(index)))))
                } else {
                    let start = facts.middle_start(&context.body, &packing);
                    Ok(Action::Continue(State::Active(context, Phase::PackMiddle(packing, start))))
                }
            }
            Phase::PackMiddle(mut packing, index) => {
                let Some(argument) = facts.middle_argument(&context.body, &packing, index) else {
                    let suffix = effects.new_arguments(packing.suffix_len).await?;
                    facts.install_suffix(&mut packing, suffix);
                    return Ok(Action::Continue(State::Active(context, Phase::PackSuffix(packing, 0))));
                };
                if let Some(provided_type) = argument.ty {
                    Ok(Action::Continue(State::Active(context, Phase::PackedMiddle(packing, index, provided_type))))
                } else {
                    let previously_in_valid_unpack_context = Some(effects.replace_flag(builder, InferenceFlags::IN_VALID_UNPACK_CONTEXT, true).await?);
                    Ok(Action::Infer {
                        pending: Pending { context, return_to: ReturnTo::PackMiddle(packing, index), previously_in_valid_unpack_context },
                        request: ChildRequest::TypeExpression(argument.node),
                    })
                }
            }
            Phase::PackedMiddle(mut packing, index, provided_type) => {
                if let Some(argument) = facts.middle_argument(&context.body, &packing, index) {
                    let flags = effects.expression_flags(builder, argument.node).await?;
                    if facts.unpack(flags)
                        && let Some(tuple) = effects.exact_tuple(provided_type).await?
                    {
                        effects.tuple_concat(builder, &mut packing.tuple_builder, tuple).await?;
                    } else if facts.unpack(flags)
                        && let Some(typevar) = facts.typevar(provided_type)
                        && facts.variadic(effects.variable_kind(typevar).await?)
                    {
                        effects.tuple_concat_typevar(builder, &mut packing.tuple_builder, typevar).await?;
                    } else {
                        effects.tuple_push(&mut packing.tuple_builder, provided_type).await?;
                    }
                }
                Ok(Action::Continue(State::Active(context, Phase::PackMiddle(packing, facts.next(index)))))
            }
            Phase::PackSuffix(mut packing, index) => {
                if let Some(argument) = facts.suffix_argument(&context.body, &packing, index) {
                    effects.pack_fixed(builder, &mut packing, argument, true).await?;
                    return Ok(Action::Continue(State::Active(context, Phase::PackSuffix(packing, facts.next(index)))));
                }
                let Packing { typevartuple_index, typevartuple_end: _, suffix_len: _, mut packed, packed_suffix, tuple_builder } = packing;
                if facts.needs_packed_tuple(&context.body, typevartuple_index) {
                    let ty = effects.finish_tuple(builder, tuple_builder).await?;
                    let argument = facts.packed_tuple_argument(&context, typevartuple_index, ty);
                    effects.push_argument(&mut packed, argument).await?;
                }
                effects.extend_arguments(&mut packed, packed_suffix).await?;
                facts.install_packed(&mut context.body, packed);
                Ok(Action::Continue(State::Active(context, Phase::ValidateStart)))
            }
            Phase::ValidateStart => {
                let types = effects.new_types(facts.variable_count(&context.body)).await?;
                facts.install_types(&mut context.body, types);
                Ok(Action::Continue(State::Active(context, Phase::Validate(0))))
            }
            Phase::Validate(index) => {
                match (facts.variable(&context.body, index), facts.argument(&context.body, index)) {
                    (Some(typevar), Some(argument)) => {
                        let default = effects.default_type(typevar).await?;
                        facts.has_default(&mut context.body, default);
                        if facts.paramspec(effects.variable_kind(typevar).await?) {
                            let provided_type = match effects.paramspec(builder, argument.node, context.body.exactly_one_paramspec).await? {
                                Ok(ty) => ty,
                                Err(()) => {
                                    facts.set_error(&mut context.body, ExplicitSpecializationError::InvalidParamSpec);
                                    effects.unknown_paramspec(builder).await?
                                }
                            };
                            facts.inferred(&mut context.body, argument.source_index, provided_type);
                            Ok(Action::Continue(State::Active(context, Phase::ValidateProvided(index, provided_type))))
                        } else if let Some(provided_type) = argument.ty {
                            Ok(Action::Continue(State::Active(context, Phase::ValidateProvided(index, provided_type))))
                        } else {
                            let previously_in_valid_unpack_context = Some(effects.replace_flag(builder, InferenceFlags::IN_VALID_UNPACK_CONTEXT, true).await?);
                            Ok(Action::Infer {
                                pending: Pending { context, return_to: ReturnTo::Validate(index), previously_in_valid_unpack_context },
                                request: ChildRequest::TypeExpression(argument.node),
                            })
                        }
                    }
                    (Some(typevar), None) => {
                        let default = effects.default_type(typevar).await?;
                        if facts.has_default(&mut context.body, default) {
                            effects.push_type(&mut context.body.specialization_types, None).await?;
                        } else {
                            // This is an error case, so no need to push into the specialization types.
                            effects.push_missing(&mut context.body.missing_typevars, typevar).await?;
                        }
                        Ok(Action::Continue(State::Active(context, Phase::Validate(facts.next(index)))))
                    }
                    (None, Some(_)) => {
                        facts.excess(&mut context.body, index);
                        Ok(Action::Continue(State::Active(context, Phase::Validate(facts.next(index)))))
                    }
                    (None, None) => Ok(Action::Continue(State::Active(context, Phase::Finalize))),
                }
            }
            Phase::ValidateProvided(index, provided_type) => {
                if let (Some(typevar), Some(argument)) = (facts.variable(&context.body, index), facts.argument(&context.body, index)) {
                    // A ParamSpec cannot be used to specialize a regular TypeVar.
                    if !facts.paramspec(effects.variable_kind(typevar).await?)
                        && let Some(tv) = facts.typevar(provided_type)
                        && facts.paramspec(effects.variable_kind(tv).await?)
                    {
                        effects.report(builder, Report::ParamSpecForTypeVar { node: argument.node, provided: tv, typevar }).await?;
                        facts.set_error(&mut context.body, ExplicitSpecializationError::ParamSpecForTypeVar);
                        effects.push_type(&mut context.body.specialization_types, Some(facts.unknown())).await?;
                        return Ok(Action::Continue(State::Active(context, Phase::Validate(facts.next(index)))));
                    }
                    // TODO consider just accepting the given specialization without checking
                    // against bounds/constraints, but recording the expression for deferred
                    // checking at end of scope. This would avoid a lot of cycles caused by eagerly
                    // doing assignment checks here.
                    let bound_or_constraints = effects.bound_or_constraints(builder, typevar).await?;
                    let type_to_check = if matches!(bound_or_constraints, Some(_)) {
                        // Defaults such as `Box[T]` may be inferred before `T` has a binding context.
                        // Bind only the copy used for validation, so the original default can later
                        // be bound to each generic that uses it.
                        effects.bind_validation(builder, provided_type).await?
                    } else { provided_type };
                    let specialization = match bound_or_constraints {
                        Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                            if effects.never_assignable(builder, facts.constraints(&context.body), type_to_check, bound).await? {
                                effects.report(builder, Report::UnsatisfiedBound { node: argument.node, provided: type_to_check, bound, typevar }).await?;
                                facts.set_error(&mut context.body, ExplicitSpecializationError::UnsatisfiedBound);
                                facts.unknown()
                            } else { provided_type }
                        }
                        Some(TypeVarBoundOrConstraints::Constraints(typevar_constraints)) => {
                            // TODO: this is wrong, the given specialization needs to be assignable
                            // to _at least one_ of the individual constraints, not to the union of
                            // all of them. `int | str` is not a valid specialization of a typevar
                            // constrained to `(int, str)`.
                            let target = effects.constraints_type(builder, typevar_constraints).await?;
                            if effects.never_assignable(builder, facts.constraints(&context.body), type_to_check, target).await? {
                                effects.report(builder, Report::UnsatisfiedConstraints { node: argument.node, provided: type_to_check, constraints: typevar_constraints, typevar }).await?;
                                facts.set_error(&mut context.body, ExplicitSpecializationError::UnsatisfiedConstraints);
                                facts.unknown()
                            } else { provided_type }
                        }
                        None => provided_type,
                    };
                    effects.push_type(&mut context.body.specialization_types, Some(specialization)).await?;
                }
                Ok(Action::Continue(State::Active(context, Phase::Validate(facts.next(index)))))
            }
            Phase::Finalize => {
                if facts.has_missing(&context.body) {
                    effects.report(builder, Report::Missing { subscript: context.request.subscript, value_ty: context.request.value_ty, variables: &context.body.missing_typevars }).await?;
                    facts.set_error(&mut context.body, ExplicitSpecializationError::MissingTypeVars);
                }
                if let Some(index) = context.body.first_excess_type_argument_index {
                    if facts.non_generic(&context.body) {
                        // Type parameter list cannot be empty, so if we reach here, `value_ty` is not a generic type.
                        effects.report(builder, Report::NonGeneric { subscript: context.request.subscript, value_ty: context.request.value_ty }).await?;
                        facts.set_error(&mut context.body, ExplicitSpecializationError::NonGeneric);
                    } else if let Some(argument) = facts.argument(&context.body, index) {
                        effects.report(builder, Report::TooMany { node: argument.node, value_ty: context.request.value_ty, typevars_len: facts.variable_count(&context.body), typevar_with_defaults: context.body.typevar_with_defaults, provided: facts.expanded_count(&context.body) }).await?;
                        facts.set_error(&mut context.body, ExplicitSpecializationError::TooManyArguments);
                    }
                }
                if context.body.store_inferred_type_arguments {
                    let inferred = facts.take_inferred(&mut context.body);
                    effects.store_inferred(builder, facts.slice(context.request.subscript), inferred).await?;
                }
                if facts.is_non_generic_error(&context.body) {
                    Ok(Action::Complete(Completed { context, result: Some(facts.unknown()) }))
                } else if facts.needs_recovery(&context.body) {
                    let variables = effects.variables(context.request.generic_context).await?;
                    facts.install_variables(&mut context.body, variables);
                    let unknowns = effects.new_types(facts.variable_count(&context.body)).await?;
                    Ok(Action::Continue(State::Active(context, Phase::Recovery(0, unknowns))))
                } else {
                    Ok(Action::Complete(Completed { context, result: None }))
                }
            }
            Phase::Recovery(index, mut unknowns) => {
                if let Some(typevar) = facts.variable(&context.body, index) {
                    let ty = if facts.paramspec(effects.variable_kind(typevar).await?) {
                        effects.unknown_paramspec(builder).await?
                    } else if facts.variadic(effects.variable_kind(typevar).await?) {
                        effects.unknown_variadic(builder).await?
                    } else { facts.unknown() };
                    effects.push_type(&mut unknowns, Some(ty)).await?;
                    Ok(Action::Continue(State::Active(context, Phase::Recovery(facts.next(index), unknowns))))
                } else {
                    facts.install_types(&mut context.body, unknowns);
                    Ok(Action::Complete(Completed { context, result: None }))
                }
            }
        }
    }

    #[synchronous(resume_sync)]
    #[capabilities(effects = ExplicitSpecializationEffects, facts = ExplicitSpecializationFacts)]
    #[passive_values(State::Active, Phase::Expanded, Phase::PackedMiddle, Phase::ValidateProvided, InferenceFlags::IN_VALID_UNPACK_CONTEXT)]
    pub(in crate::types::infer) async fn resume_with<'db, 'ast, 'expr, E: ExplicitSpecializationEffects<'db, 'ast>>(
        pending: Pending<'db, 'expr, E::Builder, E::Target>, ty: Type<'db>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: ExplicitSpecializationFacts, effects: &E,
    ) -> Result<State<'db, 'expr, E::Builder, E::Target>, E::Error> {
        let Pending { mut context, return_to, previously_in_valid_unpack_context } = pending;
        if let Some(previous) = previously_in_valid_unpack_context {
            effects.restore_flag(builder, InferenceFlags::IN_VALID_UNPACK_CONTEXT, previous).await?;
        }
        match return_to {
            ReturnTo::Expand(index) => Ok(State::Active(context, Phase::Expanded(index, ty))),
            ReturnTo::PackMiddle(packing, index) => {
                if let Some(argument) = facts.argument(&context.body, index) { facts.inferred(&mut context.body, argument.source_index, ty); }
                Ok(State::Active(context, Phase::PackedMiddle(packing, index, ty)))
            }
            ReturnTo::Validate(index) => {
                if let Some(argument) = facts.argument(&context.body, index) { facts.inferred(&mut context.body, argument.source_index, ty); }
                Ok(State::Active(context, Phase::ValidateProvided(index, ty)))
            }
        }
    }

    #[synchronous(finish_sync)]
    #[capabilities(effects = ExplicitSpecializationEffects)]
    #[passive_values(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE)]
    pub(in crate::types::infer) async fn finish_with<'db, 'ast, 'expr, E: ExplicitSpecializationEffects<'db, 'ast>>(
        completed: Completed<'db, 'expr, E::Builder, E::Target>, builder: &mut TypeInferenceBuilder<'db, 'ast>, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let Completed { context, result } = completed;
        let Context { request, body } = context;
        let ty = match result {
            Some(ty) => ty,
            None => effects.finish_target(builder, request.target, request.generic_context, &body.specialization_types).await?,
        };
        let flags = effects.retire_body(body).await?;
        effects.restore_flag(builder, InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, flags.previously_allowed_paramspec).await?;
        if let Some(previous) = flags.previously_disabled_int_float_special_case {
            effects.restore_flag(builder, InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE, previous).await?;
        }
        Ok(ty)
    }
}
