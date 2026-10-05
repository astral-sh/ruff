use crate::Db;
use crate::ProgramEnvironment;
use ruff_db::diagnostic::{Annotation, SubDiagnostic, SubDiagnosticSeverity};
use ruff_text_size::{Ranged, TextRange};

use crate::types::{
    CycleDetector, Type, UnionType, context::InferContext, diagnostic::UNSUPPORTED_BOOL_CONVERSION,
};
use ty_python_core::Truthiness;

pub(crate) mod source;
#[cfg(feature = "experimental-analysis")]
pub use source::TruthinessOperation;

impl<'db> Type<'db> {
    /// Resolves the boolean value of the type and falls back to [`Truthiness::Ambiguous`] if the type doesn't implement `__bool__` correctly.
    ///
    /// This method should only be used outside type checking or when evaluating if a type
    /// is truthy or falsy in a context where Python doesn't make an implicit `bool` call.
    /// Use [`try_bool`](Self::try_bool) for type checking or implicit `bool` calls.
    pub(crate) fn bool(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Truthiness {
        self.try_bool_impl(
            db,
            env,
            true,
            &TryBoolVisitor::new(Ok(Truthiness::Ambiguous)),
        )
        .unwrap_or_else(|err| err.fallback_truthiness())
    }

    /// Like [`Self::bool`], but returns `None` for a type equivalent to [`Type::Never`].
    ///
    /// An uninhabited type cannot produce either boolean outcome, unlike
    /// [`Truthiness::Ambiguous`]. Condition analysis uses this distinction to retain the
    /// short-circuit outcome of expressions like `flag and stop()`, where `stop` returns `Never`.
    /// The equivalence check also handles aliases and type variables bounded by `Never`.
    ///
    /// This classifies a value type, not a compound condition's evaluation. It preserves
    /// [`Self::bool`]'s error fallback and conservative handling of `__bool__` returning `Never`.
    pub(crate) fn bool_if_inhabited(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Truthiness> {
        (!self.is_equivalent_to(db, env, Type::Never)).then(|| self.bool(db, env))
    }

    /// Resolves the boolean value of a type.
    ///
    /// This is used to determine the value that would be returned
    /// when `bool(x)` is called on an object `x`.
    ///
    /// Returns an error if the type doesn't implement `__bool__` correctly.
    pub(crate) fn try_bool(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Truthiness, BoolError<'db>> {
        self.try_bool_impl(
            db,
            env,
            false,
            &TryBoolVisitor::new(Ok(Truthiness::Ambiguous)),
        )
    }

    /// Resolves the boolean value of a type.
    ///
    /// Setting `allow_short_circuit` to `true` allows the implementation to
    /// early return if the bool value of any union variant is `Truthiness::Ambiguous`.
    /// Early returning shows a 1-2% perf improvement on our benchmarks because
    /// `bool` (which doesn't care about errors) is used heavily when evaluating statically known branches.
    ///
    /// An alternative to this flag is to implement a trait similar to Rust's `Try` trait.
    /// The advantage of that is that it would allow collecting the errors as well. However,
    /// it is significantly more complex and duplicating the logic into `bool` without the error
    /// handling didn't show any significant performance difference to when using the `allow_short_circuit` flag.
    #[inline]
    fn try_bool_impl(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        allow_short_circuit: bool,
        visitor: &TryBoolVisitor<'db>,
    ) -> Result<Truthiness, BoolError<'db>> {
        source::infallible(source::try_bool_sync(
            *self,
            allow_short_circuit,
            source::BoolFacts,
            &source::OrdinaryBoolEffects { db, env, visitor },
        ))
    }
}

/// A [`CycleDetector`] that is used in `try_bool` methods.
type TryBoolVisitor<'db> =
    CycleDetector<'db, TryBool, Type<'db>, Result<Truthiness, BoolError<'db>>, 3>;
struct TryBool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoolError<'db> {
    /// The type has a `__bool__` attribute but it can't be called.
    NotCallable { not_boolable_type: Type<'db> },

    /// The type has a callable `__bool__` attribute, but it isn't callable
    /// with the given arguments.
    IncorrectArguments {
        not_boolable_type: Type<'db>,
        truthiness: Truthiness,
    },

    /// The type has a `__bool__` method, is callable with the given arguments,
    /// but the return type isn't assignable to `bool`.
    IncorrectReturnType {
        not_boolable_type: Type<'db>,
        return_type: Type<'db>,
    },

    /// A union type doesn't implement `__bool__` correctly.
    Union {
        union: UnionType<'db>,
        truthiness: Truthiness,
    },

    /// Any other reason why the type can't be converted to a bool.
    /// E.g. because calling `__bool__` returns in a union type and not all variants support `__bool__` or
    /// because `__bool__` points to a type that has a possibly missing `__call__` method.
    Other { not_boolable_type: Type<'db> },
}

impl<'db> BoolError<'db> {
    pub(super) fn fallback_truthiness(&self) -> Truthiness {
        match self {
            BoolError::NotCallable { .. }
            | BoolError::IncorrectReturnType { .. }
            | BoolError::Other { .. } => Truthiness::Ambiguous,
            BoolError::IncorrectArguments { truthiness, .. }
            | BoolError::Union { truthiness, .. } => *truthiness,
        }
    }

    fn not_boolable_type(&self) -> Type<'db> {
        match self {
            BoolError::NotCallable {
                not_boolable_type, ..
            }
            | BoolError::IncorrectArguments {
                not_boolable_type, ..
            }
            | BoolError::Other { not_boolable_type }
            | BoolError::IncorrectReturnType {
                not_boolable_type, ..
            } => *not_boolable_type,
            BoolError::Union { union, .. } => Type::Union(*union),
        }
    }

    pub(super) fn report_diagnostic(&self, context: &InferContext, condition: impl Ranged) {
        self.report_diagnostic_impl(context, condition.range());
    }

    fn report_diagnostic_impl(&self, context: &InferContext, condition: TextRange) {
        let db = context.db();
        let Some(builder) = context.report_lint(&UNSUPPORTED_BOOL_CONVERSION, condition) else {
            return;
        };
        let env = context.program_environment();
        match self {
            Self::IncorrectArguments {
                not_boolable_type, ..
            } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`",
                    not_boolable_type.display(db, env)
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "`__bool__` methods must only have a `self` parameter",
                );
                if let Some((func_span, parameter_span)) = not_boolable_type
                    .member(db, env, "__bool__")
                    .into_lookup_result(db, env)
                    .ok()
                    .and_then(|quals| quals.inner_type().parameter_span(context.db(), None))
                {
                    sub.annotate(
                        Annotation::primary(parameter_span).message("Incorrect parameters"),
                    );
                    sub.annotate(Annotation::secondary(func_span).message("Method defined here"));
                }
                diag.sub(sub);
            }
            Self::IncorrectReturnType {
                not_boolable_type,
                return_type,
            } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{not_boolable}`",
                    not_boolable = not_boolable_type.display(db, env),
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`{return_type}` is not assignable to `bool`",
                        return_type = return_type.display(db, env),
                    ),
                );
                if let Some((func_span, return_type_span)) = not_boolable_type
                    .member(db, env, "__bool__")
                    .into_lookup_result(db, env)
                    .ok()
                    .and_then(|quals| quals.inner_type().function_spans(context.db()))
                    .and_then(|spans| Some((spans.name, spans.return_type?)))
                {
                    sub.annotate(
                        Annotation::primary(return_type_span).message("Incorrect return type"),
                    );
                    sub.annotate(Annotation::secondary(func_span).message("Method defined here"));
                }
                diag.sub(sub);
            }
            Self::NotCallable { not_boolable_type } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`",
                    not_boolable_type.display(db, env)
                ));
                let sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`__bool__` on `{}` must be callable",
                        not_boolable_type.display(db, env)
                    ),
                );
                // TODO: It would be nice to create an annotation here for
                // where `__bool__` is defined. At time of writing, I couldn't
                // figure out a straight-forward way of doing this. ---AG
                diag.sub(sub);
            }
            Self::Union { union, .. } => {
                let first_error = union
                    .elements(context.db())
                    .iter()
                    .find_map(|element| element.try_bool(db, env).err())
                    .unwrap();

                builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for union `{}` \
                     because `{}` doesn't implement `__bool__` correctly",
                    Type::Union(*union).display(db, env),
                    first_error.not_boolable_type().display(db, env),
                ));
            }

            Self::Other { not_boolable_type } => {
                builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`; \
                     it incorrectly implements `__bool__`",
                    not_boolable_type.display(db, env)
                ));
            }
        }
    }
}
