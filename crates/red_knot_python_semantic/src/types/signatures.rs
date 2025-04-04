//! _Signatures_ describe the expected parameters and return type of a function or other callable.
//! Overloads and unions add complexity to this simple description.
//!
//! In a call expression, the type of the callable might be a union of several types. The call must
//! be compatible with _all_ of these types, since at runtime the callable might be an instance of
//! any of them.
//!
//! Each of the atomic types in the union must be callable. Each callable might be _overloaded_,
//! containing multiple _overload signatures_, each of which describes a different combination of
//! argument types and return types. For each callable type in the union, the call expression's
//! arguments must match _at least one_ overload.

use smallvec::{smallvec, SmallVec};

use super::{definition_expression_type, DynamicType, Type};
use crate::semantic_index::definition::Definition;
use crate::types::todo_type;
use crate::Db;
use ruff_python_ast::{self as ast, name::Name};

/// The signature of a possible union of callables.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) struct Signatures<'db> {
    /// The type that is (hopefully) callable.
    pub(crate) callable_type: Type<'db>,
    /// The type we'll use for error messages referring to details of the called signature. For calls to functions this
    /// will be the same as `callable_type`; for other callable instances it may be a `__call__` method.
    pub(crate) signature_type: Type<'db>,
    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a non-union
    /// type.
    elements: SmallVec<[CallableSignature<'db>; 1]>,
}

impl<'db> Signatures<'db> {
    pub(crate) fn not_callable(signature_type: Type<'db>) -> Self {
        Self {
            callable_type: signature_type,
            signature_type,
            elements: smallvec![CallableSignature::not_callable(signature_type)],
        }
    }

    pub(crate) fn single(signature: CallableSignature<'db>) -> Self {
        Self {
            callable_type: signature.callable_type,
            signature_type: signature.signature_type,
            elements: smallvec![signature],
        }
    }

    /// Creates a new `Signatures` from an iterator of [`Signature`]s. Panics if the iterator is
    /// empty.
    pub(crate) fn from_union<I>(signature_type: Type<'db>, elements: I) -> Self
    where
        I: IntoIterator<Item = Signatures<'db>>,
    {
        let elements: SmallVec<_> = elements
            .into_iter()
            .flat_map(|s| s.elements.into_iter())
            .collect();
        assert!(!elements.is_empty());
        Self {
            callable_type: signature_type,
            signature_type,
            elements,
        }
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, CallableSignature<'db>> {
        self.elements.iter()
    }

    pub(crate) fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
        for signature in &mut self.elements {
            signature.replace_callable_type(before, after);
        }
    }

    pub(crate) fn set_dunder_call_is_possibly_unbound(&mut self) {
        for signature in &mut self.elements {
            signature.dunder_call_is_possibly_unbound = true;
        }
    }
}

impl<'a, 'db> IntoIterator for &'a Signatures<'db> {
    type Item = &'a CallableSignature<'db>;
    type IntoIter = std::slice::Iter<'a, CallableSignature<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The signature of a single callable. If the callable is overloaded, there is a separate
/// [`Signature`] for each overload.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) struct CallableSignature<'db> {
    /// The type that is (hopefully) callable.
    pub(crate) callable_type: Type<'db>,

    /// The type we'll use for error messages referring to details of the called signature. For
    /// calls to functions this will be the same as `callable_type`; for other callable instances
    /// it may be a `__call__` method.
    pub(crate) signature_type: Type<'db>,

    /// If this is a callable object (i.e. called via a `__call__` method), the boundness of
    /// that call method.
    pub(crate) dunder_call_is_possibly_unbound: bool,

    /// The type of the bound `self` or `cls` parameter if this signature is for a bound method.
    pub(crate) bound_type: Option<Type<'db>>,

    /// The signatures of each overload of this callable. Will be empty if the type is not
    /// callable.
    ///
    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a
    /// non-overloaded callable.
    overloads: SmallVec<[Signature<'db>; 1]>,
}

impl<'db> CallableSignature<'db> {
    pub(crate) fn not_callable(signature_type: Type<'db>) -> Self {
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overloads: smallvec![],
        }
    }

    pub(crate) fn single(signature_type: Type<'db>, signature: Signature<'db>) -> Self {
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overloads: smallvec![signature],
        }
    }

    /// Creates a new `CallableSignature` from an iterator of [`Signature`]s. Returns a
    /// non-callable signature if the iterator is empty.
    pub(crate) fn from_overloads<I>(signature_type: Type<'db>, overloads: I) -> Self
    where
        I: IntoIterator<Item = Signature<'db>>,
    {
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overloads: overloads.into_iter().collect(),
        }
    }

    /// Return a signature for a dynamic callable
    pub(crate) fn dynamic(signature_type: Type<'db>) -> Self {
        let signature = Signature {
            parameters: Parameters::gradual_form(),
            return_ty: Some(signature_type),
        };
        Self::single(signature_type, signature)
    }

    /// Return a todo signature: (*args: Todo, **kwargs: Todo) -> Todo
    #[allow(unused_variables)] // 'reason' only unused in debug builds
    pub(crate) fn todo(reason: &'static str) -> Self {
        let signature_type = todo_type!(reason);
        let signature = Signature {
            parameters: Parameters::todo(),
            return_ty: Some(signature_type),
        };
        Self::single(signature_type, signature)
    }

    pub(crate) fn with_bound_type(mut self, bound_type: Type<'db>) -> Self {
        self.bound_type = Some(bound_type);
        self
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Signature<'db>> {
        self.overloads.iter()
    }

    fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
    }
}

impl<'a, 'db> IntoIterator for &'a CallableSignature<'db> {
    type Item = &'a Signature<'db>;
    type IntoIter = std::slice::Iter<'a, Signature<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The signature of one of the overloads of a callable.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub struct Signature<'db> {
    /// Parameters, in source order.
    ///
    /// The ordering of parameters in a valid signature must be: first positional-only parameters,
    /// then positional-or-keyword, then optionally the variadic parameter, then keyword-only
    /// parameters, and last, optionally the variadic keywords parameter. Parameters with defaults
    /// must come after parameters without defaults.
    ///
    /// We may get invalid signatures, though, and need to handle them without panicking.
    parameters: Parameters<'db>,

    /// Annotated return type, if any.
    pub(crate) return_ty: Option<Type<'db>>,
}

impl<'db> Signature<'db> {
    pub(crate) fn new(parameters: Parameters<'db>, return_ty: Option<Type<'db>>) -> Self {
        Self {
            parameters,
            return_ty,
        }
    }

    /// Return a todo signature: (*args: Todo, **kwargs: Todo) -> Todo
    #[allow(unused_variables)] // 'reason' only unused in debug builds
    pub(crate) fn todo(reason: &'static str) -> Self {
        Signature {
            parameters: Parameters::todo(),
            return_ty: Some(todo_type!(reason)),
        }
    }

    /// Return a typed signature from a function definition.
    pub(super) fn from_function(
        db: &'db dyn Db,
        definition: Definition<'db>,
        function_node: &ast::StmtFunctionDef,
    ) -> Self {
        let return_ty = function_node.returns.as_ref().map(|returns| {
            if function_node.is_async {
                todo_type!("generic types.CoroutineType")
            } else {
                definition_expression_type(db, definition, returns.as_ref())
            }
        });

        Self {
            parameters: Parameters::from_parameters(
                db,
                definition,
                function_node.parameters.as_ref(),
            ),
            return_ty,
        }
    }

    /// Return the parameters in this signature.
    pub(crate) fn parameters(&self) -> &Parameters<'db> {
        &self.parameters
    }

    pub(crate) fn bind_self(&self) -> Self {
        Self {
            parameters: Parameters::new(self.parameters().iter().skip(1).cloned()),
            return_ty: self.return_ty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) struct Parameters<'db> {
    // TODO: use SmallVec here once invariance bug is fixed
    value: Vec<Parameter<'db>>,

    /// Whether this parameter list represents a gradual form using `...` as the only parameter.
    ///
    /// If this is `true`, the `value` will still contain the variadic and keyword-variadic
    /// parameters. This flag is used to distinguish between an explicit `...` in the callable type
    /// as in `Callable[..., int]` and the variadic arguments in `lambda` expression as in
    /// `lambda *args, **kwargs: None`.
    ///
    /// The display implementation utilizes this flag to use `...` instead of displaying the
    /// individual variadic and keyword-variadic parameters.
    ///
    /// Note: This flag is also used to indicate invalid forms of `Callable` annotations.
    is_gradual: bool,
}

impl<'db> Parameters<'db> {
    pub(crate) fn new(parameters: impl IntoIterator<Item = Parameter<'db>>) -> Self {
        Self {
            value: parameters.into_iter().collect(),
            is_gradual: false,
        }
    }

    /// Create an empty parameter list.
    pub(crate) fn empty() -> Self {
        Self {
            value: Vec::new(),
            is_gradual: false,
        }
    }

    pub(crate) fn as_slice(&self) -> &[Parameter<'db>] {
        self.value.as_slice()
    }

    pub(crate) const fn is_gradual(&self) -> bool {
        self.is_gradual
    }

    /// Return todo parameters: (*args: Todo, **kwargs: Todo)
    pub(crate) fn todo() -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(todo_type!("todo signature *args")),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(todo_type!("todo signature **kwargs")),
            ],
            is_gradual: false,
        }
    }

    /// Return parameters that represents a gradual form using `...` as the only parameter.
    ///
    /// Internally, this is represented as `(*Any, **Any)` that accepts parameters of type [`Any`].
    ///
    /// [`Any`]: crate::types::DynamicType::Any
    pub(crate) fn gradual_form() -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Any)),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Any)),
            ],
            is_gradual: true,
        }
    }

    /// Return parameters that represents an unknown list of parameters.
    ///
    /// Internally, this is represented as `(*Unknown, **Unknown)` that accepts parameters of type
    /// [`Unknown`].
    ///
    /// [`Unknown`]: crate::types::DynamicType::Unknown
    pub(crate) fn unknown() -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Unknown)),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Unknown)),
            ],
            is_gradual: true,
        }
    }

    fn from_parameters(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &ast::Parameters,
    ) -> Self {
        let ast::Parameters {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
            range: _,
        } = parameters;
        let default_type = |param: &ast::ParameterWithDefault| {
            param
                .default()
                .map(|default| definition_expression_type(db, definition, default))
        };
        let positional_only = posonlyargs.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::PositionalOnly {
                    name: Some(arg.parameter.name.id.clone()),
                    default_type: default_type(arg),
                },
            )
        });
        let positional_or_keyword = args.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::PositionalOrKeyword {
                    name: arg.parameter.name.id.clone(),
                    default_type: default_type(arg),
                },
            )
        });
        let variadic = vararg.as_ref().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                arg,
                ParameterKind::Variadic {
                    name: arg.name.id.clone(),
                },
            )
        });
        let keyword_only = kwonlyargs.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::KeywordOnly {
                    name: arg.parameter.name.id.clone(),
                    default_type: default_type(arg),
                },
            )
        });
        let keywords = kwarg.as_ref().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                arg,
                ParameterKind::KeywordVariadic {
                    name: arg.name.id.clone(),
                },
            )
        });
        Self::new(
            positional_only
                .chain(positional_or_keyword)
                .chain(variadic)
                .chain(keyword_only)
                .chain(keywords),
        )
    }

    pub(crate) fn len(&self) -> usize {
        self.value.len()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<Parameter<'db>> {
        self.value.iter()
    }

    /// Iterate initial positional parameters, not including variadic parameter, if any.
    ///
    /// For a valid signature, this will be all positional parameters. In an invalid signature,
    /// there could be non-initial positional parameters; effectively, we just won't consider those
    /// to be positional, which is fine.
    pub(crate) fn positional(&self) -> impl Iterator<Item = &Parameter<'db>> {
        self.iter().take_while(|param| param.is_positional())
    }

    /// Return parameter at given index, or `None` if index is out-of-range.
    pub(crate) fn get(&self, index: usize) -> Option<&Parameter<'db>> {
        self.value.get(index)
    }

    /// Return positional parameter at given index, or `None` if `index` is out of range.
    ///
    /// Does not return variadic parameter.
    pub(crate) fn get_positional(&self, index: usize) -> Option<&Parameter<'db>> {
        self.get(index)
            .and_then(|parameter| parameter.is_positional().then_some(parameter))
    }

    /// Return the variadic parameter (`*args`), if any, and its index, or `None`.
    pub(crate) fn variadic(&self) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .find(|(_, parameter)| parameter.is_variadic())
    }

    /// Return parameter (with index) for given name, or `None` if no such parameter.
    ///
    /// Does not return keywords (`**kwargs`) parameter.
    ///
    /// In an invalid signature, there could be multiple parameters with the same name; we will
    /// just return the first that matches.
    pub(crate) fn keyword_by_name(&self, name: &str) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .find(|(_, parameter)| parameter.callable_by_name(name))
    }

    /// Return the keywords parameter (`**kwargs`), if any, and its index, or `None`.
    pub(crate) fn keyword_variadic(&self) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .rfind(|(_, parameter)| parameter.is_keyword_variadic())
    }
}

impl<'db, 'a> IntoIterator for &'a Parameters<'db> {
    type Item = &'a Parameter<'db>;
    type IntoIter = std::slice::Iter<'a, Parameter<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.iter()
    }
}

impl<'db> FromIterator<Parameter<'db>> for Parameters<'db> {
    fn from_iter<T: IntoIterator<Item = Parameter<'db>>>(iter: T) -> Self {
        Self::new(iter)
    }
}

impl<'db> std::ops::Index<usize> for Parameters<'db> {
    type Output = Parameter<'db>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.value[index]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) struct Parameter<'db> {
    /// Annotated type of the parameter.
    annotated_type: Option<Type<'db>>,

    kind: ParameterKind<'db>,
    pub(crate) form: ParameterForm,
}

impl<'db> Parameter<'db> {
    pub(crate) fn positional_only(name: Option<Name>) -> Self {
        Self {
            annotated_type: None,
            kind: ParameterKind::PositionalOnly {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn positional_or_keyword(name: Name) -> Self {
        Self {
            annotated_type: None,
            kind: ParameterKind::PositionalOrKeyword {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn variadic(name: Name) -> Self {
        Self {
            annotated_type: None,
            kind: ParameterKind::Variadic { name },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn keyword_only(name: Name) -> Self {
        Self {
            annotated_type: None,
            kind: ParameterKind::KeywordOnly {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn keyword_variadic(name: Name) -> Self {
        Self {
            annotated_type: None,
            kind: ParameterKind::KeywordVariadic { name },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn with_annotated_type(mut self, annotated_type: Type<'db>) -> Self {
        self.annotated_type = Some(annotated_type);
        self
    }

    pub(crate) fn with_default_type(mut self, default: Type<'db>) -> Self {
        match &mut self.kind {
            ParameterKind::PositionalOnly { default_type, .. }
            | ParameterKind::PositionalOrKeyword { default_type, .. }
            | ParameterKind::KeywordOnly { default_type, .. } => *default_type = Some(default),
            ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => {
                panic!("cannot set default value for variadic parameter")
            }
        }
        self
    }

    pub(crate) fn type_form(mut self) -> Self {
        self.form = ParameterForm::Type;
        self
    }

    /// Strip information from the parameter so that two equivalent parameters compare equal.
    /// Normalize nested unions and intersections in the annotated type, if any.
    ///
    /// See [`Type::normalized`] for more details.
    pub(crate) fn normalized(&self, db: &'db dyn Db) -> Self {
        let Parameter {
            annotated_type,
            kind,
            form,
        } = self;

        // Ensure unions and intersections are ordered in the annotated type (if there is one)
        let annotated_type = annotated_type.map(|ty| ty.normalized(db));

        // Ensure that parameter names are stripped from positional-only, variadic and keyword-variadic parameters.
        // Ensure that we only record whether a parameter *has* a default
        // (strip the precise *type* of the default from the parameter, replacing it with `Never`).
        let kind = match kind {
            ParameterKind::PositionalOnly {
                name: _,
                default_type,
            } => ParameterKind::PositionalOnly {
                name: None,
                default_type: default_type.map(|_| Type::Never),
            },
            ParameterKind::PositionalOrKeyword { name, default_type } => {
                ParameterKind::PositionalOrKeyword {
                    name: name.clone(),
                    default_type: default_type.map(|_| Type::Never),
                }
            }
            ParameterKind::KeywordOnly { name, default_type } => ParameterKind::KeywordOnly {
                name: name.clone(),
                default_type: default_type.map(|_| Type::Never),
            },
            ParameterKind::Variadic { name: _ } => ParameterKind::Variadic {
                name: Name::new_static("args"),
            },
            ParameterKind::KeywordVariadic { name: _ } => ParameterKind::KeywordVariadic {
                name: Name::new_static("kwargs"),
            },
        };

        Self {
            annotated_type,
            kind,
            form: *form,
        }
    }

    fn from_node_and_kind(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameter: &ast::Parameter,
        kind: ParameterKind<'db>,
    ) -> Self {
        Self {
            annotated_type: parameter
                .annotation()
                .map(|annotation| definition_expression_type(db, definition, annotation)),
            kind,
            form: ParameterForm::Value,
        }
    }

    /// Returns `true` if this is a keyword-only parameter.
    pub(crate) fn is_keyword_only(&self) -> bool {
        matches!(self.kind, ParameterKind::KeywordOnly { .. })
    }

    /// Returns `true` if this is a positional-only parameter.
    pub(crate) fn is_positional_only(&self) -> bool {
        matches!(self.kind, ParameterKind::PositionalOnly { .. })
    }

    /// Returns `true` if this is a variadic parameter.
    pub(crate) fn is_variadic(&self) -> bool {
        matches!(self.kind, ParameterKind::Variadic { .. })
    }

    /// Returns `true` if this is a keyword-variadic parameter.
    pub(crate) fn is_keyword_variadic(&self) -> bool {
        matches!(self.kind, ParameterKind::KeywordVariadic { .. })
    }

    /// Returns `true` if this is either a positional-only or standard (positional or keyword)
    /// parameter.
    pub(crate) fn is_positional(&self) -> bool {
        matches!(
            self.kind,
            ParameterKind::PositionalOnly { .. } | ParameterKind::PositionalOrKeyword { .. }
        )
    }

    pub(crate) fn callable_by_name(&self, name: &str) -> bool {
        match &self.kind {
            ParameterKind::PositionalOrKeyword {
                name: param_name, ..
            }
            | ParameterKind::KeywordOnly {
                name: param_name, ..
            } => param_name == name,
            _ => false,
        }
    }

    /// Annotated type of the parameter, if annotated.
    pub(crate) fn annotated_type(&self) -> Option<Type<'db>> {
        self.annotated_type
    }

    /// Kind of the parameter.
    pub(crate) fn kind(&self) -> &ParameterKind<'db> {
        &self.kind
    }

    /// Name of the parameter (if it has one).
    pub(crate) fn name(&self) -> Option<&ast::name::Name> {
        match &self.kind {
            ParameterKind::PositionalOnly { name, .. } => name.as_ref(),
            ParameterKind::PositionalOrKeyword { name, .. } => Some(name),
            ParameterKind::Variadic { name } => Some(name),
            ParameterKind::KeywordOnly { name, .. } => Some(name),
            ParameterKind::KeywordVariadic { name } => Some(name),
        }
    }

    /// Display name of the parameter, if it has one.
    pub(crate) fn display_name(&self) -> Option<ast::name::Name> {
        self.name().map(|name| match self.kind {
            ParameterKind::Variadic { .. } => ast::name::Name::new(format!("*{name}")),
            ParameterKind::KeywordVariadic { .. } => ast::name::Name::new(format!("**{name}")),
            _ => name.clone(),
        })
    }

    /// Default-value type of the parameter, if any.
    pub(crate) fn default_type(&self) -> Option<Type<'db>> {
        match self.kind {
            ParameterKind::PositionalOnly { default_type, .. }
            | ParameterKind::PositionalOrKeyword { default_type, .. }
            | ParameterKind::KeywordOnly { default_type, .. } => default_type,
            ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) enum ParameterKind<'db> {
    /// Positional-only parameter, e.g. `def f(x, /): ...`
    PositionalOnly {
        /// Parameter name.
        ///
        /// It is possible for signatures to be defined in ways that leave positional-only parameters
        /// nameless (e.g. via `Callable` annotations).
        name: Option<Name>,
        default_type: Option<Type<'db>>,
    },

    /// Positional-or-keyword parameter, e.g. `def f(x): ...`
    PositionalOrKeyword {
        /// Parameter name.
        name: Name,
        default_type: Option<Type<'db>>,
    },

    /// Variadic parameter, e.g. `def f(*args): ...`
    Variadic {
        /// Parameter name.
        name: Name,
    },

    /// Keyword-only parameter, e.g. `def f(*, x): ...`
    KeywordOnly {
        /// Parameter name.
        name: Name,
        default_type: Option<Type<'db>>,
    },

    /// Variadic keywords parameter, e.g. `def f(**kwargs): ...`
    KeywordVariadic {
        /// Parameter name.
        name: Name,
    },
}

/// Whether a parameter is used as a value or a type form.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ParameterForm {
    Value,
    Type,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{setup_db, TestDb};
    use crate::symbol::global_symbol;
    use crate::types::{FunctionType, KnownClass};
    use ruff_db::system::DbWithWritableSystem as _;

    #[track_caller]
    fn get_function_f<'db>(db: &'db TestDb, file: &'static str) -> FunctionType<'db> {
        let module = ruff_db::files::system_path_to_file(db, file).unwrap();
        global_symbol(db, module, "f")
            .symbol
            .expect_type()
            .expect_function_literal()
    }

    #[track_caller]
    fn assert_params<'db>(signature: &Signature<'db>, expected: &[Parameter<'db>]) {
        assert_eq!(signature.parameters.value.as_slice(), expected);
    }

    #[test]
    fn empty() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "def f(): ...").unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        assert!(sig.return_ty.is_none());
        assert_params(&sig, &[]);
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn full() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            from typing import Literal

            def f(a, b: int, c = 1, d: int = 2, /,
                  e = 3, f: Literal[4] = 4, *args: object,
                  g = 5, h: Literal[6] = 6, **kwargs: str) -> bytes: ...
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        assert_eq!(sig.return_ty.unwrap().display(&db).to_string(), "bytes");
        assert_params(
            &sig,
            &[
                Parameter::positional_only(Some(Name::new_static("a"))),
                Parameter::positional_only(Some(Name::new_static("b")))
                    .with_annotated_type(KnownClass::Int.to_instance(&db)),
                Parameter::positional_only(Some(Name::new_static("c")))
                    .with_default_type(Type::IntLiteral(1)),
                Parameter::positional_only(Some(Name::new_static("d")))
                    .with_annotated_type(KnownClass::Int.to_instance(&db))
                    .with_default_type(Type::IntLiteral(2)),
                Parameter::positional_or_keyword(Name::new_static("e"))
                    .with_default_type(Type::IntLiteral(3)),
                Parameter::positional_or_keyword(Name::new_static("f"))
                    .with_annotated_type(Type::IntLiteral(4))
                    .with_default_type(Type::IntLiteral(4)),
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(Type::object(&db)),
                Parameter::keyword_only(Name::new_static("g"))
                    .with_default_type(Type::IntLiteral(5)),
                Parameter::keyword_only(Name::new_static("h"))
                    .with_annotated_type(Type::IntLiteral(6))
                    .with_default_type(Type::IntLiteral(6)),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(KnownClass::Str.to_instance(&db)),
            ],
        );
    }

    #[test]
    fn not_deferred() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            class A: ...
            class B: ...

            alias = A

            def f(a: alias): ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        let [Parameter {
            annotated_type,
            kind: ParameterKind::PositionalOrKeyword { name, .. },
            ..
        }] = &sig.parameters.value[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution not deferred; we should see A not B
        assert_eq!(annotated_type.unwrap().display(&db).to_string(), "A");
    }

    #[test]
    fn deferred_in_stub() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.pyi",
            "
            class A: ...
            class B: ...

            alias = A

            def f(a: alias): ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.pyi");

        let sig = func.internal_signature(&db);

        let [Parameter {
            annotated_type,
            kind: ParameterKind::PositionalOrKeyword { name, .. },
            ..
        }] = &sig.parameters.value[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution deferred; we should see B
        assert_eq!(annotated_type.unwrap().display(&db).to_string(), "B");
    }

    #[test]
    fn generic_not_deferred() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            class A: ...
            class B: ...

            alias = A

            def f[T](a: alias, b: T) -> T: ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        let [Parameter {
            annotated_type: a_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { name: a_name, .. },
            ..
        }, Parameter {
            annotated_type: b_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { name: b_name, .. },
            ..
        }] = &sig.parameters.value[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        // TODO resolution should not be deferred; we should see A not B
        assert_eq!(
            a_annotated_ty.unwrap().display(&db).to_string(),
            "Unknown | B"
        );
        assert_eq!(b_annotated_ty.unwrap().display(&db).to_string(), "T");
    }

    #[test]
    fn generic_deferred_in_stub() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.pyi",
            "
            class A: ...
            class B: ...

            alias = A

            def f[T](a: alias, b: T) -> T: ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.pyi");

        let sig = func.internal_signature(&db);

        let [Parameter {
            annotated_type: a_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { name: a_name, .. },
            ..
        }, Parameter {
            annotated_type: b_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { name: b_name, .. },
            ..
        }] = &sig.parameters.value[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        // Parameter resolution deferred; we should see B
        assert_eq!(
            a_annotated_ty.unwrap().display(&db).to_string(),
            "Unknown | B"
        );
        assert_eq!(b_annotated_ty.unwrap().display(&db).to_string(), "T");
    }

    #[test]
    fn external_signature_no_decorator() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            def f(a: int) -> int: ...
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let expected_sig = func.internal_signature(&db);

        // With no decorators, internal and external signature are the same
        assert_eq!(func.signature(&db), &expected_sig);
    }
}
