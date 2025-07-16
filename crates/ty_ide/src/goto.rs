pub use crate::goto_declaration::goto_declaration;
pub use crate::goto_definition::goto_definition;
pub use crate::goto_type_definition::goto_type_definition;

use crate::find_node::covering_node;
use crate::stub_mapping::StubMapper;
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

#[derive(Clone, Copy, Debug)]
pub(crate) enum GotoTarget<'a> {
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

    NonLocal {
        identifier: &'a ast::Identifier,
    },
    Globals {
        identifier: &'a ast::Identifier,
    },
}

impl GotoTarget<'_> {
    pub(crate) fn inferred_type<'db>(self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
        let ty = match self {
            GotoTarget::Expression(expression) => expression.inferred_type(model),
            GotoTarget::FunctionDef(function) => function.inferred_type(model),
            GotoTarget::ClassDef(class) => class.inferred_type(model),
            GotoTarget::Parameter(parameter) => parameter.inferred_type(model),
            GotoTarget::Alias(alias) => alias.inferred_type(model),
            GotoTarget::ExceptVariable(except) => except.inferred_type(model),
            GotoTarget::KeywordArgument(argument) => {
                // TODO: Pyright resolves the declared type of the matching parameter. This seems more accurate
                // than using the inferred value.
                argument.value.inferred_type(model)
            }
            // TODO: Support identifier targets
            GotoTarget::PatternMatchRest(_)
            | GotoTarget::PatternKeywordArgument(_)
            | GotoTarget::PatternMatchStarName(_)
            | GotoTarget::PatternMatchAsName(_)
            | GotoTarget::ImportedModule(_)
            | GotoTarget::TypeParamTypeVarName(_)
            | GotoTarget::TypeParamParamSpecName(_)
            | GotoTarget::TypeParamTypeVarTupleName(_)
            | GotoTarget::NonLocal { .. }
            | GotoTarget::Globals { .. } => return None,
        };

        Some(ty)
    }

    /// Gets the navigation ranges for this goto target.
    /// If a stub mapper is provided, definitions from stub files will be mapped to
    /// their corresponding source file implementations.
    pub(crate) fn get_definition_targets(
        self,
        file: ruff_db::files::File,
        db: &dyn crate::Db,
        stub_mapper: Option<&StubMapper>,
    ) -> Option<crate::NavigationTargets> {
        use crate::NavigationTarget;
        use ruff_python_ast as ast;

        match self {
            // For names, find the definitions of the symbol
            GotoTarget::Expression(expression) => {
                if let ast::ExprRef::Name(name) = expression {
                    Self::get_name_definition_targets(name, file, db, stub_mapper)
                } else {
                    // For other expressions, we can't find definitions
                    None
                }
            }

            // For already-defined symbols, they are their own definitions
            GotoTarget::FunctionDef(function) => {
                let range = function.name.range;
                Some(crate::NavigationTargets::single(NavigationTarget {
                    file,
                    focus_range: range,
                    full_range: function.range(),
                }))
            }

            GotoTarget::ClassDef(class) => {
                let range = class.name.range;
                Some(crate::NavigationTargets::single(NavigationTarget {
                    file,
                    focus_range: range,
                    full_range: class.range(),
                }))
            }

            GotoTarget::Parameter(parameter) => {
                let range = parameter.name.range;
                Some(crate::NavigationTargets::single(NavigationTarget {
                    file,
                    focus_range: range,
                    full_range: parameter.range(),
                }))
            }

            // For imports, find the symbol being imported
            GotoTarget::Alias(_alias) => {
                // For aliases, we don't have the ExprName node, so we can't get the scope
                // For now, return None. In the future, we could look up the imported symbol
                None
            }

            // TODO: Handle attribute and method accesses (y in `x.y` expressions)
            // TODO: Handle keyword arguments in call expression
            // TODO: Handle multi-part module names in import statements
            // TODO: Handle imported symbol in y in `from x import y as z` statement
            // TODO: Handle string literals that map to TypedDict fields
            _ => None,
        }
    }

    /// Get navigation targets for definitions associated with a name expression
    fn get_name_definition_targets(
        name: &ruff_python_ast::ExprName,
        file: ruff_db::files::File,
        db: &dyn crate::Db,
        stub_mapper: Option<&StubMapper>,
    ) -> Option<crate::NavigationTargets> {
        use ty_python_semantic::definitions_for_name;

        // Get all definitions for this name
        let mut definitions = definitions_for_name(db, file, name);

        // Apply stub mapping if a mapper is provided
        if let Some(mapper) = stub_mapper {
            definitions = mapper.map_definitions(definitions);
        }

        if definitions.is_empty() {
            return None;
        }

        // Convert definitions to navigation targets
        let targets = convert_resolved_definitions_to_targets(db, definitions);

        Some(crate::NavigationTargets::unique(targets))
    }
}

impl Ranged for GotoTarget<'_> {
    fn range(&self) -> TextRange {
        match self {
            GotoTarget::Expression(expression) => expression.range(),
            GotoTarget::FunctionDef(function) => function.name.range,
            GotoTarget::ClassDef(class) => class.name.range,
            GotoTarget::Parameter(parameter) => parameter.name.range,
            GotoTarget::Alias(alias) => alias.name.range,
            GotoTarget::ImportedModule(module) => module.module.as_ref().unwrap().range,
            GotoTarget::ExceptVariable(except) => except.name.as_ref().unwrap().range,
            GotoTarget::KeywordArgument(keyword) => keyword.arg.as_ref().unwrap().range,
            GotoTarget::PatternMatchRest(rest) => rest.rest.as_ref().unwrap().range,
            GotoTarget::PatternKeywordArgument(keyword) => keyword.attr.range,
            GotoTarget::PatternMatchStarName(star) => star.name.as_ref().unwrap().range,
            GotoTarget::PatternMatchAsName(as_name) => as_name.name.as_ref().unwrap().range,
            GotoTarget::TypeParamTypeVarName(type_var) => type_var.name.range,
            GotoTarget::TypeParamParamSpecName(spec) => spec.name.range,
            GotoTarget::TypeParamTypeVarTupleName(tuple) => tuple.name.range,
            GotoTarget::NonLocal { identifier, .. } => identifier.range,
            GotoTarget::Globals { identifier, .. } => identifier.range,
        }
    }
}

/// Converts a collection of `ResolvedDefinition` items into `NavigationTarget` items.
fn convert_resolved_definitions_to_targets(
    db: &dyn crate::Db,
    definitions: Vec<ty_python_semantic::ResolvedDefinition<'_>>,
) -> Vec<crate::NavigationTarget> {
    definitions
        .into_iter()
        .map(|resolved_definition| match resolved_definition {
            ty_python_semantic::ResolvedDefinition::Definition(definition) => {
                // Get the parsed module for range calculation
                let definition_file = definition.file(db);
                let module = ruff_db::parsed::parsed_module(db, definition_file).load(db);

                // Get the ranges for this definition
                let focus_range = definition.focus_range(db, &module);
                let full_range = definition.full_range(db, &module);

                crate::NavigationTarget {
                    file: focus_range.file(),
                    focus_range: focus_range.range(),
                    full_range: full_range.range(),
                }
            }
            ty_python_semantic::ResolvedDefinition::ModuleFile(module_file) => {
                // For module files, navigate to the beginning of the file
                crate::NavigationTarget {
                    file: module_file,
                    focus_range: ruff_text_size::TextRange::default(), // Start of file
                    full_range: ruff_text_size::TextRange::default(),  // Start of file
                }
            }
        })
        .collect()
}

pub(crate) fn find_goto_target(
    parsed: &ParsedModuleRef,
    offset: TextSize,
) -> Option<GotoTarget<'_>> {
    let token = parsed
        .tokens()
        .at_offset(offset)
        .max_by_key(|token| match token.kind() {
            TokenKind::Name
            | TokenKind::String
            | TokenKind::Complex
            | TokenKind::Float
            | TokenKind::Int => 1,
            _ => 0,
        })?;

    let covering_node = covering_node(parsed.syntax().into(), token.range())
        .find_first(|node| node.is_identifier() || node.is_expression())
        .ok()?;

    tracing::trace!("Covering node is of kind {:?}", covering_node.node().kind());

    match covering_node.node() {
        AnyNodeRef::Identifier(identifier) => match covering_node.parent() {
            Some(AnyNodeRef::StmtFunctionDef(function)) => Some(GotoTarget::FunctionDef(function)),
            Some(AnyNodeRef::StmtClassDef(class)) => Some(GotoTarget::ClassDef(class)),
            Some(AnyNodeRef::Parameter(parameter)) => Some(GotoTarget::Parameter(parameter)),
            Some(AnyNodeRef::Alias(alias)) => Some(GotoTarget::Alias(alias)),
            Some(AnyNodeRef::StmtImportFrom(from)) => Some(GotoTarget::ImportedModule(from)),
            Some(AnyNodeRef::ExceptHandlerExceptHandler(handler)) => {
                Some(GotoTarget::ExceptVariable(handler))
            }
            Some(AnyNodeRef::Keyword(keyword)) => Some(GotoTarget::KeywordArgument(keyword)),
            Some(AnyNodeRef::PatternMatchMapping(mapping)) => {
                Some(GotoTarget::PatternMatchRest(mapping))
            }
            Some(AnyNodeRef::PatternKeyword(keyword)) => {
                Some(GotoTarget::PatternKeywordArgument(keyword))
            }
            Some(AnyNodeRef::PatternMatchStar(star)) => {
                Some(GotoTarget::PatternMatchStarName(star))
            }
            Some(AnyNodeRef::PatternMatchAs(as_pattern)) => {
                Some(GotoTarget::PatternMatchAsName(as_pattern))
            }
            Some(AnyNodeRef::TypeParamTypeVar(var)) => Some(GotoTarget::TypeParamTypeVarName(var)),
            Some(AnyNodeRef::TypeParamParamSpec(bound)) => {
                Some(GotoTarget::TypeParamParamSpecName(bound))
            }
            Some(AnyNodeRef::TypeParamTypeVarTuple(var_tuple)) => {
                Some(GotoTarget::TypeParamTypeVarTupleName(var_tuple))
            }
            Some(AnyNodeRef::ExprAttribute(attribute)) => {
                Some(GotoTarget::Expression(attribute.into()))
            }
            Some(AnyNodeRef::StmtNonlocal(_)) => Some(GotoTarget::NonLocal { identifier }),
            Some(AnyNodeRef::StmtGlobal(_)) => Some(GotoTarget::Globals { identifier }),
            None => None,
            Some(parent) => {
                tracing::debug!(
                    "Missing `GoToTarget` for identifier with parent {:?}",
                    parent.kind()
                );
                None
            }
        },

        node => node.as_expr_ref().map(GotoTarget::Expression),
    }
}
