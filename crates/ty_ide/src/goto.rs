use crate::docstring::Docstring;
pub use crate::goto_declaration::goto_declaration;
pub use crate::goto_definition::goto_definition;
pub use crate::goto_type_definition::goto_type_definition;

use std::borrow::Cow;

use crate::find_node::covering_node;
use crate::stub_mapping::StubMapper;
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_parser::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};

use ty_python_semantic::ResolvedDefinition;
use ty_python_semantic::types::Type;
use ty_python_semantic::types::ide_support::{
    call_signature_details, definitions_for_keyword_argument,
};
use ty_python_semantic::{
    HasDefinition, HasType, ImportAliasResolution, SemanticModel, definitions_for_imported_symbol,
    definitions_for_name,
};

#[derive(Clone, Debug)]
pub(crate) enum GotoTarget<'a> {
    Expression(ast::ExprRef<'a>),
    FunctionDef(&'a ast::StmtFunctionDef),
    ClassDef(&'a ast::StmtClassDef),
    Parameter(&'a ast::Parameter),

    /// Go to on the operator of a binary operation.
    ///
    /// ```py
    /// a + b
    ///   ^
    /// ```
    BinOp {
        expression: &'a ast::ExprBinOp,
        operator_range: TextRange,
    },

    /// Go to where the operator of a unary operation is defined.
    ///
    /// ```py
    /// -a
    /// ^
    /// ```
    UnaryOp {
        expression: &'a ast::ExprUnaryOp,
        operator_range: TextRange,
    },

    /// Multi-part module names
    /// Handles both `import foo.bar` and `from foo.bar import baz` cases
    /// ```py
    /// import foo.bar
    ///        ^^^
    /// from foo.bar import baz
    ///          ^^^
    /// ```
    ImportModuleComponent {
        module_name: String,
        component_index: usize,
        component_range: TextRange,
    },

    /// Import alias in standard import statement
    /// ```py
    /// import foo.bar as baz
    ///                   ^^^
    /// ```
    ImportModuleAlias {
        alias: &'a ast::Alias,
    },

    /// Import alias in from import statement
    /// ```py
    /// from foo import bar as baz
    ///                 ^^^
    /// from foo import bar as baz
    ///                        ^^^
    /// ```
    ImportSymbolAlias {
        alias: &'a ast::Alias,
        range: TextRange,
        import_from: &'a ast::StmtImportFrom,
    },

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
    KeywordArgument {
        keyword: &'a ast::Keyword,
        call_expression: &'a ast::ExprCall,
    },

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
    /// Go to on the invocation of a callable
    ///
    /// ```py
    /// x = mymodule.MyClass(1, 2)
    ///              ^^^^^^^
    /// ```
    ///
    /// This is equivalent to `GotoTarget::Expression(callable)` but enriched
    /// with information about the actual callable implementation.
    ///
    /// That is, if you click on `MyClass` in `MyClass()` it is *both* a
    /// reference to the class and to the initializer of the class. Therefore
    /// it would be ideal for goto-* and docstrings to be some intelligent
    /// merging of both the class and the initializer.
    Call {
        /// The callable that can actually be selected by a cursor
        callable: ast::ExprRef<'a>,
        /// The call of the callable
        call: &'a ast::ExprCall,
    },
}

/// The resolved definitions for a `GotoTarget`
#[derive(Debug, Clone)]
pub(crate) enum DefinitionsOrTargets<'db> {
    /// We computed actual Definitions we can do followup queries on.
    Definitions(Vec<ResolvedDefinition<'db>>),
    /// We directly computed a navigation.
    ///
    /// We can't get docs or usefully compute goto-definition for this.
    Targets(crate::NavigationTargets),
}

impl<'db> DefinitionsOrTargets<'db> {
    pub(crate) fn from_ty(db: &'db dyn crate::Db, ty: Type<'db>) -> Option<Self> {
        let ty_def = ty.definition(db)?;
        let resolved = match ty_def {
            ty_python_semantic::types::TypeDefinition::Module(module) => {
                ResolvedDefinition::Module(module.file(db)?)
            }
            ty_python_semantic::types::TypeDefinition::Class(definition) => {
                ResolvedDefinition::Definition(definition)
            }
            ty_python_semantic::types::TypeDefinition::Function(definition) => {
                ResolvedDefinition::Definition(definition)
            }
            ty_python_semantic::types::TypeDefinition::TypeVar(definition) => {
                ResolvedDefinition::Definition(definition)
            }
            ty_python_semantic::types::TypeDefinition::TypeAlias(definition) => {
                ResolvedDefinition::Definition(definition)
            }
        };
        Some(DefinitionsOrTargets::Definitions(vec![resolved]))
    }

    /// Get the "goto-declaration" interpretation of this definition
    ///
    /// In this case it basically returns exactly what was found.
    pub(crate) fn declaration_targets(
        self,
        db: &'db dyn crate::Db,
    ) -> Option<crate::NavigationTargets> {
        match self {
            DefinitionsOrTargets::Definitions(definitions) => {
                definitions_to_navigation_targets(db, None, definitions)
            }
            DefinitionsOrTargets::Targets(targets) => Some(targets),
        }
    }

    /// Get the "goto-definition" interpretation of this definition
    ///
    /// In this case we apply stub-mapping to try to find the "real" implementation
    /// if the definition we have is found in a stub file.
    pub(crate) fn definition_targets(
        self,
        db: &'db dyn crate::Db,
    ) -> Option<crate::NavigationTargets> {
        match self {
            DefinitionsOrTargets::Definitions(definitions) => {
                definitions_to_navigation_targets(db, Some(&StubMapper::new(db)), definitions)
            }
            DefinitionsOrTargets::Targets(targets) => Some(targets),
        }
    }

    /// Get the docstring for this definition
    ///
    /// Typically documentation only appears on implementations and not stubs,
    /// so this will check both the goto-declarations and goto-definitions (in that order)
    /// and return the first one found.
    pub(crate) fn docstring(self, db: &'db dyn crate::Db) -> Option<Docstring> {
        let definitions = match self {
            DefinitionsOrTargets::Definitions(definitions) => definitions,
            // Can't find docs for these
            // (make more cases DefinitionOrTargets::Definitions to get more docs!)
            DefinitionsOrTargets::Targets(_) => return None,
        };
        for definition in &definitions {
            // If we got a docstring from the original definition, use it
            if let Some(docstring) = definition.docstring(db) {
                return Some(Docstring::new(docstring));
            }
        }

        // If the definition is located within a stub file and no docstring
        // is present, try to map the symbol to an implementation file and extract
        // the docstring from that location.
        let stub_mapper = StubMapper::new(db);

        // Try to find the corresponding implementation definition
        for definition in stub_mapper.map_definitions(definitions) {
            if let Some(docstring) = definition.docstring(db) {
                return Some(Docstring::new(docstring));
            }
        }

        None
    }
}

impl GotoTarget<'_> {
    pub(crate) fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
        let ty = match self {
            GotoTarget::Expression(expression) => expression.inferred_type(model),
            GotoTarget::FunctionDef(function) => function.inferred_type(model),
            GotoTarget::ClassDef(class) => class.inferred_type(model),
            GotoTarget::Parameter(parameter) => parameter.inferred_type(model),
            GotoTarget::ImportSymbolAlias { alias, .. } => alias.inferred_type(model),
            GotoTarget::ImportModuleAlias { alias } => alias.inferred_type(model),
            GotoTarget::ExceptVariable(except) => except.inferred_type(model),
            GotoTarget::KeywordArgument { keyword, .. } => keyword.value.inferred_type(model),
            // When asking the type of a callable, usually you want the callable itself?
            // (i.e. the type of `MyClass` in `MyClass()` is `<class MyClass>` and not `() -> MyClass`)
            GotoTarget::Call { callable, .. } => callable.inferred_type(model),
            GotoTarget::TypeParamTypeVarName(typevar) => typevar.inferred_type(model),
            // TODO: Support identifier targets
            GotoTarget::PatternMatchRest(_)
            | GotoTarget::PatternKeywordArgument(_)
            | GotoTarget::PatternMatchStarName(_)
            | GotoTarget::PatternMatchAsName(_)
            | GotoTarget::ImportModuleComponent { .. }
            | GotoTarget::TypeParamParamSpecName(_)
            | GotoTarget::TypeParamTypeVarTupleName(_)
            | GotoTarget::NonLocal { .. }
            | GotoTarget::Globals { .. } => return None,
            GotoTarget::BinOp { expression, .. } => {
                let (_, ty) =
                    ty_python_semantic::definitions_for_bin_op(model.db(), model, expression)?;
                ty
            }
            GotoTarget::UnaryOp { expression, .. } => {
                let (_, ty) =
                    ty_python_semantic::definitions_for_unary_op(model.db(), model, expression)?;
                ty
            }
        };

        Some(ty)
    }

    /// Gets the definitions for this goto target.
    ///
    /// The `alias_resolution` parameter controls whether import aliases
    /// (i.e. "x" in "from a import b as x") are resolved or returned as is.
    /// We want to resolve them in some cases (like "goto declaration") but not in others
    /// (like find references or rename).
    ///
    ///
    /// Ideally this would always return `DefinitionsOrTargets::Definitions`
    /// as this is more useful for doing stub mapping (goto-definition) and
    /// retrieving docstrings. However for now some cases are stubbed out
    /// as just returning a raw `NavigationTarget`.
    pub(crate) fn get_definition_targets<'db>(
        &self,
        file: ruff_db::files::File,
        db: &'db dyn crate::Db,
        alias_resolution: ImportAliasResolution,
    ) -> Option<DefinitionsOrTargets<'db>> {
        use crate::NavigationTarget;

        match self {
            GotoTarget::Expression(expression) => definitions_for_expression(db, file, expression)
                .map(DefinitionsOrTargets::Definitions),

            // For already-defined symbols, they are their own definitions
            GotoTarget::FunctionDef(function) => {
                let model = SemanticModel::new(db, file);
                Some(DefinitionsOrTargets::Definitions(vec![
                    ResolvedDefinition::Definition(function.definition(&model)),
                ]))
            }

            GotoTarget::ClassDef(class) => {
                let model = SemanticModel::new(db, file);
                Some(DefinitionsOrTargets::Definitions(vec![
                    ResolvedDefinition::Definition(class.definition(&model)),
                ]))
            }

            GotoTarget::Parameter(parameter) => {
                let model = SemanticModel::new(db, file);
                Some(DefinitionsOrTargets::Definitions(vec![
                    ResolvedDefinition::Definition(parameter.definition(&model)),
                ]))
            }

            // For import aliases (offset within 'y' or 'z' in "from x import y as z")
            GotoTarget::ImportSymbolAlias {
                alias, import_from, ..
            } => {
                let symbol_name = alias.name.as_str();
                Some(DefinitionsOrTargets::Definitions(
                    definitions_for_imported_symbol(
                        db,
                        file,
                        import_from,
                        symbol_name,
                        alias_resolution,
                    ),
                ))
            }

            GotoTarget::ImportModuleComponent {
                module_name,
                component_index,
                ..
            } => {
                // Handle both `import foo.bar` and `from foo.bar import baz` where offset is within module component
                let components: Vec<&str> = module_name.split('.').collect();

                // Build the module name up to and including the component containing the offset
                let target_module_name = components[..=*component_index].join(".");

                // Try to resolve the module
                definitions_for_module(db, &target_module_name)
            }

            // Handle import aliases (offset within 'z' in "import x.y as z")
            GotoTarget::ImportModuleAlias { alias } => {
                if alias_resolution == ImportAliasResolution::ResolveAliases {
                    let full_module_name = alias.name.as_str();
                    // Try to resolve the module
                    definitions_for_module(db, full_module_name)
                } else {
                    let alias_range = alias.asname.as_ref().unwrap().range;
                    Some(DefinitionsOrTargets::Targets(
                        crate::NavigationTargets::single(NavigationTarget {
                            file,
                            focus_range: alias_range,
                            full_range: alias.range(),
                        }),
                    ))
                }
            }

            // Handle keyword arguments in call expressions
            GotoTarget::KeywordArgument {
                keyword,
                call_expression,
            } => Some(DefinitionsOrTargets::Definitions(
                definitions_for_keyword_argument(db, file, keyword, call_expression),
            )),

            // For exception variables, they are their own definitions (like parameters)
            GotoTarget::ExceptVariable(except_handler) => {
                let model = SemanticModel::new(db, file);
                Some(DefinitionsOrTargets::Definitions(vec![
                    ResolvedDefinition::Definition(except_handler.definition(&model)),
                ]))
            }

            // For pattern match rest variables, they are their own definitions
            GotoTarget::PatternMatchRest(pattern_mapping) => {
                if let Some(rest_name) = &pattern_mapping.rest {
                    let range = rest_name.range;
                    Some(DefinitionsOrTargets::Targets(
                        crate::NavigationTargets::single(NavigationTarget::new(file, range)),
                    ))
                } else {
                    None
                }
            }

            // For pattern match as names, they are their own definitions
            GotoTarget::PatternMatchAsName(pattern_as) => {
                if let Some(name) = &pattern_as.name {
                    let range = name.range;
                    Some(DefinitionsOrTargets::Targets(
                        crate::NavigationTargets::single(NavigationTarget::new(file, range)),
                    ))
                } else {
                    None
                }
            }

            // For callables, both the definition of the callable and the actual function impl are relevant.
            //
            // Prefer the function impl over the callable so that its docstrings win if defined.
            GotoTarget::Call { callable, call } => {
                let mut definitions = definitions_for_callable(db, file, call);
                let expr_definitions =
                    definitions_for_expression(db, file, callable).unwrap_or_default();
                definitions.extend(expr_definitions);

                if definitions.is_empty() {
                    None
                } else {
                    Some(DefinitionsOrTargets::Definitions(definitions))
                }
            }

            GotoTarget::BinOp { expression, .. } => {
                let model = SemanticModel::new(db, file);

                let (definitions, _) =
                    ty_python_semantic::definitions_for_bin_op(db, &model, expression)?;

                Some(DefinitionsOrTargets::Definitions(definitions))
            }

            GotoTarget::UnaryOp { expression, .. } => {
                let model = SemanticModel::new(db, file);
                let (definitions, _) =
                    ty_python_semantic::definitions_for_unary_op(db, &model, expression)?;

                Some(DefinitionsOrTargets::Definitions(definitions))
            }

            _ => None,
        }
    }

    /// Returns the text representation of this goto target.
    /// Returns `None` if no meaningful string representation can be provided.
    /// This is used by the "references" feature, which looks for references
    /// to this goto target.
    pub(crate) fn to_string(&self) -> Option<Cow<'_, str>> {
        match self {
            GotoTarget::Call {
                callable: expression,
                ..
            }
            | GotoTarget::Expression(expression) => match expression {
                ast::ExprRef::Name(name) => Some(Cow::Borrowed(name.id.as_str())),
                ast::ExprRef::Attribute(attr) => Some(Cow::Borrowed(attr.attr.as_str())),
                _ => None,
            },
            GotoTarget::FunctionDef(function) => Some(Cow::Borrowed(function.name.as_str())),
            GotoTarget::ClassDef(class) => Some(Cow::Borrowed(class.name.as_str())),
            GotoTarget::Parameter(parameter) => Some(Cow::Borrowed(parameter.name.as_str())),
            GotoTarget::ImportSymbolAlias { alias, .. } => {
                if let Some(asname) = &alias.asname {
                    Some(Cow::Borrowed(asname.as_str()))
                } else {
                    Some(Cow::Borrowed(alias.name.as_str()))
                }
            }
            GotoTarget::ImportModuleComponent {
                module_name,
                component_index,
                ..
            } => {
                let components: Vec<&str> = module_name.split('.').collect();
                if let Some(component) = components.get(*component_index) {
                    Some(Cow::Borrowed(*component))
                } else {
                    Some(Cow::Borrowed(module_name))
                }
            }
            GotoTarget::ImportModuleAlias { alias } => {
                if let Some(asname) = &alias.asname {
                    Some(Cow::Borrowed(asname.as_str()))
                } else {
                    Some(Cow::Borrowed(alias.name.as_str()))
                }
            }
            GotoTarget::ExceptVariable(except) => {
                Some(Cow::Borrowed(except.name.as_ref()?.as_str()))
            }
            GotoTarget::KeywordArgument { keyword, .. } => {
                Some(Cow::Borrowed(keyword.arg.as_ref()?.as_str()))
            }
            GotoTarget::PatternMatchRest(rest) => Some(Cow::Borrowed(rest.rest.as_ref()?.as_str())),
            GotoTarget::PatternKeywordArgument(keyword) => {
                Some(Cow::Borrowed(keyword.attr.as_str()))
            }
            GotoTarget::PatternMatchStarName(star) => {
                Some(Cow::Borrowed(star.name.as_ref()?.as_str()))
            }
            GotoTarget::PatternMatchAsName(as_name) => {
                Some(Cow::Borrowed(as_name.name.as_ref()?.as_str()))
            }
            GotoTarget::TypeParamTypeVarName(type_var) => {
                Some(Cow::Borrowed(type_var.name.as_str()))
            }
            GotoTarget::TypeParamParamSpecName(spec) => Some(Cow::Borrowed(spec.name.as_str())),
            GotoTarget::TypeParamTypeVarTupleName(tuple) => {
                Some(Cow::Borrowed(tuple.name.as_str()))
            }
            GotoTarget::NonLocal { identifier, .. } => Some(Cow::Borrowed(identifier.as_str())),
            GotoTarget::Globals { identifier, .. } => Some(Cow::Borrowed(identifier.as_str())),
            GotoTarget::BinOp { .. } | GotoTarget::UnaryOp { .. } => None,
        }
    }

    /// Creates a `GotoTarget` from a `CoveringNode` and an offset within the node
    pub(crate) fn from_covering_node<'a>(
        covering_node: &crate::find_node::CoveringNode<'a>,
        offset: TextSize,
        tokens: &Tokens,
    ) -> Option<GotoTarget<'a>> {
        tracing::trace!("Covering node is of kind {:?}", covering_node.node().kind());

        match covering_node.node() {
            AnyNodeRef::Identifier(identifier) => match covering_node.parent() {
                Some(AnyNodeRef::StmtFunctionDef(function)) => {
                    Some(GotoTarget::FunctionDef(function))
                }
                Some(AnyNodeRef::StmtClassDef(class)) => Some(GotoTarget::ClassDef(class)),
                Some(AnyNodeRef::Parameter(parameter)) => Some(GotoTarget::Parameter(parameter)),
                Some(AnyNodeRef::Alias(alias)) => {
                    // Find the containing import statement to determine the type
                    let import_stmt = covering_node.ancestors().find(|node| {
                        matches!(
                            node,
                            AnyNodeRef::StmtImport(_) | AnyNodeRef::StmtImportFrom(_)
                        )
                    });

                    match import_stmt {
                        Some(AnyNodeRef::StmtImport(_)) => {
                            // Regular import statement like "import x.y as z"

                            // Is the offset within the alias name (asname) part?
                            if let Some(asname) = &alias.asname {
                                if asname.range.contains_inclusive(offset) {
                                    return Some(GotoTarget::ImportModuleAlias { alias });
                                }
                            }

                            // Is the offset in the module name part?
                            if alias.name.range.contains_inclusive(offset) {
                                let full_name = alias.name.as_str();

                                if let Some((component_index, component_range)) =
                                    find_module_component(
                                        full_name,
                                        alias.name.range.start(),
                                        offset,
                                    )
                                {
                                    return Some(GotoTarget::ImportModuleComponent {
                                        module_name: full_name.to_string(),
                                        component_index,
                                        component_range,
                                    });
                                }
                            }

                            None
                        }
                        Some(AnyNodeRef::StmtImportFrom(import_from)) => {
                            // From import statement like "from x import y as z"

                            // Is the offset within the alias name (asname) part?
                            if let Some(asname) = &alias.asname {
                                if asname.range.contains_inclusive(offset) {
                                    return Some(GotoTarget::ImportSymbolAlias {
                                        alias,
                                        range: asname.range,
                                        import_from,
                                    });
                                }
                            }

                            // Is the offset in the original name part?
                            if alias.name.range.contains_inclusive(offset) {
                                return Some(GotoTarget::ImportSymbolAlias {
                                    alias,
                                    range: alias.name.range,
                                    import_from,
                                });
                            }

                            None
                        }
                        _ => None,
                    }
                }
                Some(AnyNodeRef::StmtImportFrom(from)) => {
                    // Handle offset within module name in from import statements
                    if let Some(module_expr) = &from.module {
                        let full_module_name = module_expr.to_string();

                        if let Some((component_index, component_range)) = find_module_component(
                            &full_module_name,
                            module_expr.range.start(),
                            offset,
                        ) {
                            return Some(GotoTarget::ImportModuleComponent {
                                module_name: full_module_name,
                                component_index,
                                component_range,
                            });
                        }
                    }

                    None
                }
                Some(AnyNodeRef::ExceptHandlerExceptHandler(handler)) => {
                    Some(GotoTarget::ExceptVariable(handler))
                }
                Some(AnyNodeRef::Keyword(keyword)) => {
                    // Find the containing call expression from the ancestor chain
                    let call_expression = covering_node
                        .ancestors()
                        .find_map(ruff_python_ast::AnyNodeRef::expr_call)?;
                    Some(GotoTarget::KeywordArgument {
                        keyword,
                        call_expression,
                    })
                }
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
                Some(AnyNodeRef::TypeParamTypeVar(var)) => {
                    Some(GotoTarget::TypeParamTypeVarName(var))
                }
                Some(AnyNodeRef::TypeParamParamSpec(bound)) => {
                    Some(GotoTarget::TypeParamParamSpecName(bound))
                }
                Some(AnyNodeRef::TypeParamTypeVarTuple(var_tuple)) => {
                    Some(GotoTarget::TypeParamTypeVarTupleName(var_tuple))
                }
                Some(AnyNodeRef::ExprAttribute(attribute)) => {
                    // Check if this is seemingly a callable being invoked (the `y` in `x.y(...)`)
                    let grandparent_expr = covering_node.ancestors().nth(2);
                    let attribute_expr = attribute.into();
                    if let Some(AnyNodeRef::ExprCall(call)) = grandparent_expr {
                        if ruff_python_ast::ExprRef::from(&call.func) == attribute_expr {
                            return Some(GotoTarget::Call {
                                call,
                                callable: attribute_expr,
                            });
                        }
                    }
                    Some(GotoTarget::Expression(attribute_expr))
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

            AnyNodeRef::ExprBinOp(binary) => {
                if offset >= binary.left.end() && offset < binary.right.start() {
                    let between_operands =
                        tokens.in_range(TextRange::new(binary.left.end(), binary.right.start()));
                    if let Some(operator_token) = between_operands
                        .iter()
                        .find(|token| token.kind().as_binary_operator().is_some())
                        && operator_token.range().contains_inclusive(offset)
                    {
                        return Some(GotoTarget::BinOp {
                            expression: binary,
                            operator_range: operator_token.range(),
                        });
                    }
                }

                Some(GotoTarget::Expression(binary.into()))
            }

            AnyNodeRef::ExprUnaryOp(unary) => {
                if offset >= unary.start() && offset < unary.operand.start() {
                    let before_operand =
                        tokens.in_range(TextRange::new(unary.start(), unary.operand.start()));

                    if let Some(operator_token) = before_operand
                        .iter()
                        .find(|token| token.kind().as_unary_operator().is_some())
                        && operator_token.range().contains_inclusive(offset)
                    {
                        return Some(GotoTarget::UnaryOp {
                            expression: unary,
                            operator_range: operator_token.range(),
                        });
                    }
                }
                Some(GotoTarget::Expression(unary.into()))
            }

            node => {
                // Check if this is seemingly a callable being invoked (the `x` in `x(...)`)
                let parent = covering_node.parent();
                if let (Some(AnyNodeRef::ExprCall(call)), AnyNodeRef::ExprName(name)) =
                    (parent, node)
                {
                    return Some(GotoTarget::Call {
                        call,
                        callable: name.into(),
                    });
                }
                node.as_expr_ref().map(GotoTarget::Expression)
            }
        }
    }
}

impl Ranged for GotoTarget<'_> {
    fn range(&self) -> TextRange {
        match self {
            GotoTarget::Call {
                callable: expression,
                ..
            }
            | GotoTarget::Expression(expression) => match expression {
                ast::ExprRef::Attribute(attribute) => attribute.attr.range,
                _ => expression.range(),
            },
            GotoTarget::FunctionDef(function) => function.name.range,
            GotoTarget::ClassDef(class) => class.name.range,
            GotoTarget::Parameter(parameter) => parameter.name.range,
            GotoTarget::ImportSymbolAlias { range, .. } => *range,
            GotoTarget::ImportModuleComponent {
                component_range, ..
            } => *component_range,
            GotoTarget::ImportModuleAlias { alias } => alias.asname.as_ref().unwrap().range,
            GotoTarget::ExceptVariable(except) => except.name.as_ref().unwrap().range,
            GotoTarget::KeywordArgument { keyword, .. } => keyword.arg.as_ref().unwrap().range,
            GotoTarget::PatternMatchRest(rest) => rest.rest.as_ref().unwrap().range,
            GotoTarget::PatternKeywordArgument(keyword) => keyword.attr.range,
            GotoTarget::PatternMatchStarName(star) => star.name.as_ref().unwrap().range,
            GotoTarget::PatternMatchAsName(as_name) => as_name.name.as_ref().unwrap().range,
            GotoTarget::TypeParamTypeVarName(type_var) => type_var.name.range,
            GotoTarget::TypeParamParamSpecName(spec) => spec.name.range,
            GotoTarget::TypeParamTypeVarTupleName(tuple) => tuple.name.range,
            GotoTarget::NonLocal { identifier, .. } => identifier.range,
            GotoTarget::Globals { identifier, .. } => identifier.range,
            GotoTarget::BinOp { operator_range, .. }
            | GotoTarget::UnaryOp { operator_range, .. } => *operator_range,
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
            ty_python_semantic::ResolvedDefinition::Module(file) => {
                // For modules, navigate to the start of the file
                crate::NavigationTarget::new(file, TextRange::default())
            }
            ty_python_semantic::ResolvedDefinition::FileWithRange(file_range) => {
                // For file ranges, navigate to the specific range within the file
                crate::NavigationTarget::new(file_range.file(), file_range.range())
            }
        })
        .collect()
}

/// Shared helper to get definitions for an expr (that is presumably a name/attr)
fn definitions_for_expression<'db>(
    db: &'db dyn crate::Db,
    file: ruff_db::files::File,
    expression: &ruff_python_ast::ExprRef<'_>,
) -> Option<Vec<ResolvedDefinition<'db>>> {
    match expression {
        ast::ExprRef::Name(name) => Some(definitions_for_name(db, file, name)),
        ast::ExprRef::Attribute(attribute) => Some(ty_python_semantic::definitions_for_attribute(
            db, file, attribute,
        )),
        _ => None,
    }
}

fn definitions_for_callable<'db>(
    db: &'db dyn crate::Db,
    file: ruff_db::files::File,
    call: &ast::ExprCall,
) -> Vec<ResolvedDefinition<'db>> {
    let model = SemanticModel::new(db, file);
    // Attempt to refine to a specific call
    let signature_info = call_signature_details(db, &model, call);
    signature_info
        .into_iter()
        .filter_map(|signature| signature.definition.map(ResolvedDefinition::Definition))
        .collect()
}

/// Shared helper to map and convert resolved definitions into navigation targets.
fn definitions_to_navigation_targets<'db>(
    db: &dyn crate::Db,
    stub_mapper: Option<&StubMapper<'db>>,
    mut definitions: Vec<ty_python_semantic::ResolvedDefinition<'db>>,
) -> Option<crate::NavigationTargets> {
    if let Some(mapper) = stub_mapper {
        definitions = mapper.map_definitions(definitions);
    }
    if definitions.is_empty() {
        None
    } else {
        let targets = convert_resolved_definitions_to_targets(db, definitions);
        Some(crate::NavigationTargets::unique(targets))
    }
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

            TokenKind::Comment => -1,

            // if we have a<CURSOR>+b`, prefer the `+` token (by respecting the token ordering)
            // This matches VS Code's behavior where it sends the start of the clicked token as offset.
            kind if kind.as_binary_operator().is_some() || kind.as_unary_operator().is_some() => 1,
            _ => 0,
        })?;

    if token.kind().is_comment() {
        return None;
    }

    let covering_node = covering_node(parsed.syntax().into(), token.range())
        .find_first(|node| node.is_identifier() || node.is_expression())
        .ok()?;

    GotoTarget::from_covering_node(&covering_node, offset, parsed.tokens())
}

/// Helper function to resolve a module name and create a navigation target.
fn definitions_for_module<'db>(
    db: &'db dyn crate::Db,
    module_name_str: &str,
) -> Option<DefinitionsOrTargets<'db>> {
    use ty_python_semantic::{ModuleName, resolve_module};

    if let Some(module_name) = ModuleName::new(module_name_str) {
        if let Some(resolved_module) = resolve_module(db, &module_name) {
            if let Some(module_file) = resolved_module.file(db) {
                return Some(DefinitionsOrTargets::Definitions(vec![
                    ResolvedDefinition::Module(module_file),
                ]));
            }
        }
    }
    None
}

/// Helper function to extract module component information from a dotted module name
fn find_module_component(
    full_module_name: &str,
    module_start: TextSize,
    offset: TextSize,
) -> Option<(usize, TextRange)> {
    let pos_in_module = offset - module_start;
    let pos_in_module = pos_in_module.to_usize();

    // Split the module name into components and find which one contains the offset
    let mut current_pos = 0;
    let components: Vec<&str> = full_module_name.split('.').collect();

    for (i, component) in components.iter().enumerate() {
        let component_start = current_pos;
        let component_end = current_pos + component.len();

        // Check if the offset is within this component or at its right boundary
        if pos_in_module >= component_start && pos_in_module <= component_end {
            let component_range = TextRange::new(
                module_start + TextSize::from(u32::try_from(component_start).ok()?),
                module_start + TextSize::from(u32::try_from(component_end).ok()?),
            );
            return Some((i, component_range));
        }

        // Move past this component and the dot
        current_pos = component_end + 1; // +1 for the dot
    }

    None
}
