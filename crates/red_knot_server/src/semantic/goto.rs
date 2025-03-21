use crate::document::{FileRangeExt, ToRangeExt};
use crate::find_node::covering_node;
use crate::system::file_to_url;
use crate::PositionEncoding;
use lsp_types::Location;
use red_knot_python_semantic::types::{
    ClassLiteralType, FunctionType, InstanceType, KnownInstanceType, ModuleLiteralType, Type,
};
use red_knot_python_semantic::{Db, HasType, SemanticModel};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange, TextSize};

pub(crate) fn go_to_type_definition(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangeInfo<NavigationTargets>> {
    let root = parsed_module(db.upcast(), file);
    let go_to_target = find_go_to_target(root.syntax().into(), offset)?;

    let model = SemanticModel::new(db, file);

    let ty = match go_to_target {
        GoToTarget::Expression(expression) => expression.inferred_type(&model),
        GoToTarget::FunctionDef(function) => function.inferred_type(&model),
        GoToTarget::ClassDef(class) => class.inferred_type(&model),
        GoToTarget::Parameter(parameter) => parameter.inferred_type(&model),
        GoToTarget::Alias(alias) => alias.inferred_type(&model),
        GoToTarget::ExceptVariable(except) => except.inferred_type(&model),
        GoToTarget::KeywordArgument(argument) => {
            // TODO: Pyright resolves the declared type of the matching parameter. This seems more accurate
            // than using the inferred value.
            argument.value.inferred_type(&model)
        }

        // TODO: Better support for go to type definition in match pattern.
        // This may require improving type inference (e.g. it currently doesn't handle `...rest`)
        // but it also requires a new API to query the type because implementing `HasType` for `PatternMatchMapping`
        // is ambiguous.
        GoToTarget::PatternMatchRest(_)
        | GoToTarget::PatternKeywordArgument(_)
        | GoToTarget::PatternMatchStarName(_)
        | GoToTarget::PatternMatchAsName(_) => return None,

        // TODO: Resolve the module; The type inference already does all the work
        // but type isn't stored anywhere. We should either extract the logic
        // for resolving the module from a ImportFromStmt or store the type during semantic analysis
        GoToTarget::ImportedModule(_) => return None,

        // Targets without a type definition.
        GoToTarget::TypeParamTypeVarName(_)
        | GoToTarget::TypeParamParamSpecName(_)
        | GoToTarget::TypeParamTypeVarTupleName(_) => return None,
    };

    tracing::debug!("Inferred type of covering node is {}", ty.display(db));

    Some(RangeInfo {
        range: FileRange::new(file, go_to_target.range()),
        info: ty.navigation_targets(db),
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GoToTarget<'a> {
    Expression(ast::ExprRef<'a>),
    FunctionDef(&'a ast::StmtFunctionDef),
    ClassDef(&'a ast::StmtClassDef),
    Parameter(&'a ast::Parameter),
    Alias(&'a ast::Alias),

    /// Go to on the module name of an import from
    /// ```py
    /// from foo import bar
    ///      ^^^
    /// ```
    ImportedModule(&'a ast::StmtImportFrom),

    /// Go to on the exception handler variable
    /// ```py
    /// try: ...
    /// except Exception as e: ...
    ///                     ^
    /// ```
    ExceptVariable(&'a ast::ExceptHandlerExceptHandler),

    /// Go to on a keyword argument
    /// ```py
    /// test(a = 1)
    ///      ^
    /// ```
    KeywordArgument(&'a ast::Keyword),

    /// Go to on the rest parameter of a pattern match
    ///
    /// ```py
    /// match x:
    ///     case {"a": a, "b": b, **rest}: ...
    ///                             ^^^^
    /// ```
    PatternMatchRest(&'a ast::PatternMatchMapping),

    /// Go to on a keyword argument of a class pattern
    ///
    /// ```py
    /// match Point3D(0, 0, 0):
    ///     case Point3D(x=0, y=0, z=0): ...
    ///                  ^    ^    ^
    /// ```
    PatternKeywordArgument(&'a ast::PatternKeyword),

    /// Go to on a pattern star argument
    ///
    /// ```py
    /// match array:
    ///     case [*args]: ...
    ///            ^^^^
    PatternMatchStarName(&'a ast::PatternMatchStar),

    /// Go to on the name of a pattern match as pattern
    ///
    /// ```py
    /// match x:
    ///     case [x] as y: ...
    ///                 ^
    PatternMatchAsName(&'a ast::PatternMatchAs),

    /// Go to on the name of a type variable
    ///
    /// ```py
    /// type Alias[T: int = bool] = list[T]
    ///            ^
    /// ```
    TypeParamTypeVarName(&'a ast::TypeParamTypeVar),

    /// Go to on the name of a type param spec
    ///
    /// ```py
    /// type Alias[**P = [int, str]] = Callable[P, int]
    ///              ^
    /// ```
    TypeParamParamSpecName(&'a ast::TypeParamParamSpec),

    /// Go to on the name of a type var tuple
    ///
    /// ```py
    /// type Alias[*Ts = ()] = tuple[*Ts]
    ///             ^^
    /// ```
    TypeParamTypeVarTupleName(&'a ast::TypeParamTypeVarTuple),
}

impl Ranged for GoToTarget<'_> {
    fn range(&self) -> TextRange {
        match self {
            GoToTarget::Expression(expression) => expression.range(),
            GoToTarget::FunctionDef(function) => function.name.range,
            GoToTarget::ClassDef(class) => class.name.range,
            GoToTarget::Parameter(parameter) => parameter.name.range,
            GoToTarget::Alias(alias) => alias.name.range,
            GoToTarget::ImportedModule(module) => module.module.as_ref().unwrap().range,
            GoToTarget::ExceptVariable(except) => except.name.as_ref().unwrap().range,
            GoToTarget::KeywordArgument(keyword) => keyword.arg.as_ref().unwrap().range,
            GoToTarget::PatternMatchRest(rest) => rest.rest.as_ref().unwrap().range,
            GoToTarget::PatternKeywordArgument(keyword) => keyword.attr.range,
            GoToTarget::PatternMatchStarName(star) => star.name.as_ref().unwrap().range,
            GoToTarget::PatternMatchAsName(as_name) => as_name.name.as_ref().unwrap().range,
            GoToTarget::TypeParamTypeVarName(type_var) => type_var.name.range,
            GoToTarget::TypeParamParamSpecName(spec) => spec.name.range,
            GoToTarget::TypeParamTypeVarTupleName(tuple) => tuple.name.range,
        }
    }
}

pub(crate) fn find_go_to_target(root: AnyNodeRef, offset: TextSize) -> Option<GoToTarget> {
    let covering_node = covering_node(root, TextRange::empty(offset));
    tracing::trace!("Covering node is of kind {:?}", covering_node.node().kind());

    match covering_node.node() {
        AnyNodeRef::Identifier(_) => match covering_node.parent() {
            Some(AnyNodeRef::StmtFunctionDef(function)) => Some(GoToTarget::FunctionDef(function)),
            Some(AnyNodeRef::StmtClassDef(class)) => Some(GoToTarget::ClassDef(class)),
            Some(AnyNodeRef::Parameter(parameter)) => Some(GoToTarget::Parameter(parameter)),
            Some(AnyNodeRef::Alias(alias)) => Some(GoToTarget::Alias(alias)),
            Some(AnyNodeRef::StmtImportFrom(from)) => Some(GoToTarget::ImportedModule(from)),
            Some(AnyNodeRef::ExceptHandlerExceptHandler(handler)) => {
                Some(GoToTarget::ExceptVariable(handler))
            }
            Some(AnyNodeRef::Keyword(keyword)) => Some(GoToTarget::KeywordArgument(keyword)),
            Some(AnyNodeRef::PatternMatchMapping(mapping)) => {
                Some(GoToTarget::PatternMatchRest(mapping))
            }
            Some(AnyNodeRef::PatternKeyword(keyword)) => {
                Some(GoToTarget::PatternKeywordArgument(keyword))
            }
            Some(AnyNodeRef::PatternMatchStar(star)) => {
                Some(GoToTarget::PatternMatchStarName(star))
            }
            Some(AnyNodeRef::PatternMatchAs(as_pattern)) => {
                Some(GoToTarget::PatternMatchAsName(as_pattern))
            }
            Some(AnyNodeRef::TypeParamTypeVar(var)) => Some(GoToTarget::TypeParamTypeVarName(var)),
            Some(AnyNodeRef::TypeParamParamSpec(bound)) => {
                Some(GoToTarget::TypeParamParamSpecName(bound))
            }
            Some(AnyNodeRef::TypeParamTypeVarTuple(var_tuple)) => {
                Some(GoToTarget::TypeParamTypeVarTupleName(var_tuple))
            }
            None => None,
            Some(parent) => {
                tracing::debug!(
                    "Missing `GoToTarget` for identifier with parent {:?}",
                    parent.kind()
                );
                None
            }
        },
        node => node.as_expr_ref().map(GoToTarget::Expression),
    }
}

/// Information associated with a text range.
pub(crate) struct RangeInfo<T> {
    pub range: FileRange,
    pub info: T,
}

/// Target to which the editor can navigate to.
#[derive(Debug, Clone)]
pub(crate) struct NavigationTarget {
    file: File,

    /// The range that should be focused when navigating to the target.
    ///
    /// This is typically not the full range of the node. For example, it's the range of the class's name in a class definition.
    ///
    /// The `focus_range` must be fully covered by `full_range`.
    focus_range: TextRange,

    /// The range covering the entire target.
    full_range: TextRange,
}

impl NavigationTarget {
    pub(crate) fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location> {
        FileRange::new(self.file, self.focus_range).to_location(db, encoding)
    }

    pub(crate) fn to_link(
        &self,
        db: &dyn Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink> {
        let uri = file_to_url(db, self.file)?;
        let source = source_text(db.upcast(), self.file);
        let index = line_index(db.upcast(), self.file);

        let target_range = self.full_range.to_range(&source, &index, encoding);
        let selection_range = self.focus_range.to_range(&source, &index, encoding);

        let src = src.map(|src| {
            let source = source_text(db.upcast(), src.file());
            let index = line_index(db.upcast(), src.file());

            src.range().to_range(&source, &index, encoding)
        });

        Some(lsp_types::LocationLink {
            target_uri: uri,
            target_range,
            target_selection_range: selection_range,
            origin_selection_range: src,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NavigationTargets(smallvec::SmallVec<[NavigationTarget; 1]>);

impl NavigationTargets {
    fn single(target: NavigationTarget) -> Self {
        Self(smallvec::smallvec![target])
    }

    fn empty() -> Self {
        Self(smallvec::SmallVec::new())
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

impl IntoIterator for NavigationTargets {
    type Item = NavigationTarget;
    type IntoIter = smallvec::IntoIter<[NavigationTarget; 1]>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<NavigationTarget> for NavigationTargets {
    fn from_iter<T: IntoIterator<Item = NavigationTarget>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

trait HasNavigationTargets {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets;
}

impl HasNavigationTargets for Type<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        match self {
            Type::FunctionLiteral(function) => function.navigation_targets(db),
            Type::ModuleLiteral(module) => module.navigation_targets(db),
            Type::Union(union) => union
                .iter(db)
                .flat_map(|target| target.navigation_targets(db))
                .collect(),
            Type::ClassLiteral(class) => class.navigation_targets(db),
            Type::Instance(instance) => instance.navigation_targets(db),
            Type::KnownInstance(instance) => instance.navigation_targets(db),
            Type::Dynamic(_)
            | Type::SubclassOf(_)
            | Type::Never
            | Type::Callable(_)
            | Type::Intersection(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::SliceLiteral(_)
            | Type::Tuple(_) => NavigationTargets::empty(),
        }
    }
}

impl HasNavigationTargets for FunctionType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let function_range = self.focus_range(db);
        NavigationTargets::single(NavigationTarget {
            file: function_range.file(),
            focus_range: function_range.range(),
            full_range: self.full_range(db).range(),
        })
    }
}

impl HasNavigationTargets for ClassLiteralType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let class = self.class();
        let class_range = class.focus_range(db);
        NavigationTargets::single(NavigationTarget {
            file: class_range.file(),
            focus_range: class_range.range(),
            full_range: class.full_range(db).range(),
        })
    }
}

impl HasNavigationTargets for InstanceType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let class = self.class();
        let class_range = class.focus_range(db);
        NavigationTargets::single(NavigationTarget {
            file: class_range.file(),
            focus_range: class_range.range(),
            full_range: class.full_range(db).range(),
        })
    }
}

impl HasNavigationTargets for ModuleLiteralType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let file = self.module(db).file();

        NavigationTargets::single(NavigationTarget {
            file,
            focus_range: TextRange::default(),
            full_range: TextRange::default(),
        })
    }
}

impl HasNavigationTargets for KnownInstanceType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        match self {
            KnownInstanceType::TypeVar(var) => {
                let range = var.range(db);
                NavigationTargets::single(NavigationTarget {
                    file: range.file(),
                    focus_range: range.range(),
                    full_range: range.range(),
                })
            }

            // TODO: Resolve the target from typeshed
            KnownInstanceType::Annotated
            | KnownInstanceType::Literal
            | KnownInstanceType::LiteralString
            | KnownInstanceType::Optional
            | KnownInstanceType::Union
            | KnownInstanceType::NoReturn
            | KnownInstanceType::Never
            | KnownInstanceType::Any
            | KnownInstanceType::Tuple
            | KnownInstanceType::List
            | KnownInstanceType::Dict
            | KnownInstanceType::Set
            | KnownInstanceType::FrozenSet
            | KnownInstanceType::ChainMap
            | KnownInstanceType::Counter
            | KnownInstanceType::DefaultDict
            | KnownInstanceType::Deque
            | KnownInstanceType::OrderedDict
            | KnownInstanceType::Protocol
            | KnownInstanceType::Type
            | KnownInstanceType::TypeAliasType(_)
            | KnownInstanceType::Unknown
            | KnownInstanceType::AlwaysTruthy
            | KnownInstanceType::AlwaysFalsy
            | KnownInstanceType::Not
            | KnownInstanceType::Intersection
            | KnownInstanceType::TypeOf
            | KnownInstanceType::CallableTypeFromFunction
            | KnownInstanceType::TypingSelf
            | KnownInstanceType::Final
            | KnownInstanceType::ClassVar
            | KnownInstanceType::Callable
            | KnownInstanceType::Concatenate
            | KnownInstanceType::Unpack
            | KnownInstanceType::Required
            | KnownInstanceType::NotRequired
            | KnownInstanceType::TypeAlias
            | KnownInstanceType::TypeGuard
            | KnownInstanceType::TypeIs
            | KnownInstanceType::ReadOnly => NavigationTargets::empty(),
        }
    }
}
