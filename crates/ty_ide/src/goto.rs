use crate::docstring::Docstring;
pub use crate::goto_declaration::goto_declaration;
pub use crate::goto_definition::goto_definition;
pub use crate::goto_type_definition::goto_type_definition;

use std::borrow::Cow;

use crate::stub_mapping::StubMapper;
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::find_node::{CoveringNode, covering_node};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange, TextSize};

use ty_python_semantic::ResolvedDefinition;
use ty_python_semantic::types::Type;
use ty_python_semantic::types::ide_support::{
    call_signature_details, call_type_simplified_by_overloads, definitions_for_keyword_argument,
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
        level: u32,
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
        asname: &'a ast::Identifier,
    },

    /// In an import statement, the named under which the symbol is exported
    /// in the imported file.
    ///
    /// ```py
    /// from foo import bar as baz
    ///                 ^^^
    /// ```
    ImportExportedName {
        alias: &'a ast::Alias,
        import_from: &'a ast::StmtImportFrom,
    },

    /// Import alias in from import statement
    /// ```py
    /// from foo import bar as baz
    ///                        ^^^
    /// ```
    ImportSymbolAlias {
        alias: &'a ast::Alias,
        asname: &'a ast::Identifier,
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

    /// Go to on a sub-expression of a string annotation's sub-AST
    ///
    /// ```py
    /// x: "int | None"
    ///           ^^^^
    /// ```
    ///
    /// This is equivalent to `GotoTarget::Expression` but the expression
    /// isn't actually in the AST.
    StringAnnotationSubexpr {
        /// The string literal that is a string annotation.
        string_expr: &'a ast::ExprStringLiteral,
        /// The range to query in the sub-AST for the sub-expression.
        subrange: TextRange,
        /// If the expression is a Name of some kind this is the name (just a cached result).
        name: Option<String>,
    },
}

/// The resolved definitions for a `GotoTarget`
#[derive(Debug, Clone)]
pub(crate) struct Definitions<'db>(pub Vec<ResolvedDefinition<'db>>);

impl<'db> Definitions<'db> {
    pub(crate) fn from_ty(db: &'db dyn crate::Db, ty: Type<'db>) -> Option<Self> {
        let ty_def = ty.definition(db)?;
        let resolved = match ty_def {
            ty_python_semantic::types::TypeDefinition::Module(module) => {
                ResolvedDefinition::Module(module.file(db)?)
            }
            ty_python_semantic::types::TypeDefinition::Class(definition)
            | ty_python_semantic::types::TypeDefinition::Function(definition)
            | ty_python_semantic::types::TypeDefinition::TypeVar(definition)
            | ty_python_semantic::types::TypeDefinition::TypeAlias(definition)
            | ty_python_semantic::types::TypeDefinition::SpecialForm(definition)
            | ty_python_semantic::types::TypeDefinition::NewType(definition) => {
                ResolvedDefinition::Definition(definition)
            }
        };
        Some(Definitions(vec![resolved]))
    }

    /// Get the "goto-declaration" interpretation of this definition
    ///
    /// In this case it basically returns exactly what was found.
    pub(crate) fn declaration_targets(
        self,
        db: &'db dyn ty_python_semantic::Db,
    ) -> Option<crate::NavigationTargets> {
        definitions_to_navigation_targets(db, None, self.0)
    }

    /// Get the "goto-definition" interpretation of this definition
    ///
    /// In this case we apply stub-mapping to try to find the "real" implementation
    /// if the definition we have is found in a stub file.
    pub(crate) fn definition_targets(
        self,
        db: &'db dyn ty_python_semantic::Db,
    ) -> Option<crate::NavigationTargets> {
        definitions_to_navigation_targets(db, Some(&StubMapper::new(db)), self.0)
    }

    /// Get the docstring for this definition
    ///
    /// Typically documentation only appears on implementations and not stubs,
    /// so this will check both the goto-declarations and goto-definitions (in that order)
    /// and return the first one found.
    pub(crate) fn docstring(self, db: &'db dyn crate::Db) -> Option<Docstring> {
        for definition in &self.0 {
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
        for definition in stub_mapper.map_definitions(self.0) {
            if let Some(docstring) = definition.docstring(db) {
                return Some(Docstring::new(docstring));
            }
        }

        None
    }
}

impl GotoTarget<'_> {
    pub(crate) fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
        match self {
            GotoTarget::Expression(expression) => expression.inferred_type(model),
            GotoTarget::FunctionDef(function) => function.inferred_type(model),
            GotoTarget::ClassDef(class) => class.inferred_type(model),
            GotoTarget::Parameter(parameter) => parameter.inferred_type(model),
            GotoTarget::ImportSymbolAlias { alias, .. }
            | GotoTarget::ImportModuleAlias { alias, .. }
            | GotoTarget::ImportExportedName { alias, .. } => alias.inferred_type(model),
            GotoTarget::ExceptVariable(except) => except.inferred_type(model),
            GotoTarget::KeywordArgument { keyword, .. } => keyword.value.inferred_type(model),
            // When asking the type of a callable, usually you want the callable itself?
            // (i.e. the type of `MyClass` in `MyClass()` is `<class MyClass>` and not `() -> MyClass`)
            GotoTarget::Call { callable, .. } => callable.inferred_type(model),
            GotoTarget::TypeParamTypeVarName(typevar) => typevar.inferred_type(model),
            GotoTarget::ImportModuleComponent {
                module_name,
                component_index,
                level,
                ..
            } => {
                // We don't currently support hovering the bare `.` so there is always a name
                let module = import_name(module_name, *component_index);
                model.resolve_module_type(Some(module), *level)
            }
            GotoTarget::StringAnnotationSubexpr {
                string_expr,
                subrange,
                ..
            } => {
                let (subast, _submodel) = model.enter_string_annotation(string_expr)?;
                let submod = subast.syntax();
                let subnode = covering_node(submod.into(), *subrange).node();

                // The type checker knows the type of the full annotation but nothing else
                if AnyNodeRef::from(&*submod.body) == subnode {
                    string_expr.inferred_type(model)
                } else {
                    // TODO: force the typechecker to tell us its secrets
                    // (it computes but then immediately discards these types)
                    None
                }
            }
            GotoTarget::BinOp { expression, .. } => {
                let (_, ty) = ty_python_semantic::definitions_for_bin_op(model, expression)?;
                Some(ty)
            }
            GotoTarget::UnaryOp { expression, .. } => {
                let (_, ty) = ty_python_semantic::definitions_for_unary_op(model, expression)?;
                Some(ty)
            }
            // TODO: Support identifier targets
            GotoTarget::PatternMatchRest(_)
            | GotoTarget::PatternKeywordArgument(_)
            | GotoTarget::PatternMatchStarName(_)
            | GotoTarget::PatternMatchAsName(_)
            | GotoTarget::TypeParamParamSpecName(_)
            | GotoTarget::TypeParamTypeVarTupleName(_)
            | GotoTarget::NonLocal { .. }
            | GotoTarget::Globals { .. } => None,
        }
    }

    /// Try to get a simplified display of this callable type by resolving overloads
    pub(crate) fn call_type_simplified_by_overloads(
        &self,
        model: &SemanticModel,
    ) -> Option<String> {
        if let GotoTarget::Call { call, .. } = self {
            call_type_simplified_by_overloads(model, call)
        } else {
            None
        }
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
        model: &SemanticModel<'db>,
        alias_resolution: ImportAliasResolution,
    ) -> Option<Definitions<'db>> {
        let definitions = match self {
            GotoTarget::Expression(expression) => {
                definitions_for_expression(model, *expression, alias_resolution)
            }
            // For already-defined symbols, they are their own definitions
            GotoTarget::FunctionDef(function) => Some(vec![ResolvedDefinition::Definition(
                function.definition(model),
            )]),

            GotoTarget::ClassDef(class) => Some(vec![ResolvedDefinition::Definition(
                class.definition(model),
            )]),

            GotoTarget::Parameter(parameter) => Some(vec![ResolvedDefinition::Definition(
                parameter.definition(model),
            )]),

            // For import aliases (offset within 'y' or 'z' in "from x import y as z")
            GotoTarget::ImportSymbolAlias { asname, .. } => Some(definitions_for_name(
                model,
                asname.as_str(),
                AnyNodeRef::from(*asname),
                alias_resolution,
            )),

            GotoTarget::ImportExportedName { alias, import_from } => {
                let symbol_name = alias.name.as_str();
                Some(definitions_for_imported_symbol(
                    model,
                    import_from,
                    symbol_name,
                    alias_resolution,
                ))
            }

            GotoTarget::ImportModuleComponent {
                module_name,
                component_index,
                level,
                ..
            } => {
                // We don't currently support hovering the bare `.` so there is always a name
                let module = import_name(module_name, *component_index);
                definitions_for_module(model, Some(module), *level)
            }

            // Handle import aliases (offset within 'z' in "import x.y as z")
            GotoTarget::ImportModuleAlias { asname, .. } => Some(definitions_for_name(
                model,
                asname.as_str(),
                AnyNodeRef::from(*asname),
                alias_resolution,
            )),

            // Handle keyword arguments in call expressions
            GotoTarget::KeywordArgument {
                keyword,
                call_expression,
            } => Some(definitions_for_keyword_argument(
                model,
                keyword,
                call_expression,
            )),

            // For exception variables, they are their own definitions (like parameters)
            GotoTarget::ExceptVariable(except_handler) => {
                Some(vec![ResolvedDefinition::Definition(
                    except_handler.definition(model),
                )])
            }

            // Patterns are glorified assignments but we have to look them up by ident
            // because they're not expressions
            GotoTarget::PatternMatchRest(pattern_mapping) => {
                pattern_mapping.rest.as_ref().map(|name| {
                    definitions_for_name(
                        model,
                        name.as_str(),
                        AnyNodeRef::Identifier(name),
                        alias_resolution,
                    )
                })
            }

            GotoTarget::PatternMatchAsName(pattern_as) => pattern_as.name.as_ref().map(|name| {
                definitions_for_name(
                    model,
                    name.as_str(),
                    AnyNodeRef::Identifier(name),
                    alias_resolution,
                )
            }),

            GotoTarget::PatternKeywordArgument(pattern_keyword) => {
                let name = &pattern_keyword.attr;
                Some(definitions_for_name(
                    model,
                    name.as_str(),
                    AnyNodeRef::Identifier(name),
                    alias_resolution,
                ))
            }

            GotoTarget::PatternMatchStarName(pattern_star) => {
                pattern_star.name.as_ref().map(|name| {
                    definitions_for_name(
                        model,
                        name.as_str(),
                        AnyNodeRef::Identifier(name),
                        alias_resolution,
                    )
                })
            }

            // For callables, both the definition of the callable and the actual function impl are relevant.
            //
            // Prefer the function impl over the callable so that its docstrings win if defined.
            GotoTarget::Call { callable, call } => {
                let mut definitions = Vec::new();

                // We prefer the specific overload for hover, go-to-def etc. However,
                // `definitions_for_callable` always resolves import aliases. That's why we
                // skip it in cases import alias resolution is turned of (rename, highlight references).
                if alias_resolution == ImportAliasResolution::ResolveAliases {
                    definitions.extend(definitions_for_callable(model, call));
                }

                let expr_definitions =
                    definitions_for_expression(model, *callable, alias_resolution)
                        .unwrap_or_default();
                definitions.extend(expr_definitions);

                if definitions.is_empty() {
                    None
                } else {
                    Some(definitions)
                }
            }

            GotoTarget::BinOp { expression, .. } => {
                let (definitions, _) =
                    ty_python_semantic::definitions_for_bin_op(model, expression)?;

                Some(definitions)
            }

            GotoTarget::UnaryOp { expression, .. } => {
                let (definitions, _) =
                    ty_python_semantic::definitions_for_unary_op(model, expression)?;

                Some(definitions)
            }

            // String annotations sub-expressions require us to recurse into the sub-AST
            GotoTarget::StringAnnotationSubexpr {
                string_expr,
                subrange,
                ..
            } => {
                let (subast, submodel) = model.enter_string_annotation(string_expr)?;
                let subexpr = covering_node(subast.syntax().into(), *subrange)
                    .node()
                    .as_expr_ref()?;
                definitions_for_expression(&submodel, subexpr, alias_resolution)
            }

            // nonlocal and global are essentially loads, but again they're statements,
            // so we need to look them up by ident
            GotoTarget::NonLocal { identifier } | GotoTarget::Globals { identifier } => {
                Some(definitions_for_name(
                    model,
                    identifier.as_str(),
                    AnyNodeRef::Identifier(identifier),
                    alias_resolution,
                ))
            }

            // These are declarations of sorts, but they're stmts and not exprs, so look up by ident.
            GotoTarget::TypeParamTypeVarName(type_var) => {
                let name = &type_var.name;
                Some(definitions_for_name(
                    model,
                    name.as_str(),
                    AnyNodeRef::Identifier(name),
                    alias_resolution,
                ))
            }

            GotoTarget::TypeParamParamSpecName(name) => {
                let name = &name.name;
                Some(definitions_for_name(
                    model,
                    name.as_str(),
                    AnyNodeRef::Identifier(name),
                    alias_resolution,
                ))
            }

            GotoTarget::TypeParamTypeVarTupleName(name) => {
                let name = &name.name;
                Some(definitions_for_name(
                    model,
                    name.as_str(),
                    AnyNodeRef::Identifier(name),
                    alias_resolution,
                ))
            }
        };
        definitions.map(Definitions)
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
            GotoTarget::StringAnnotationSubexpr { name, .. } => name.as_deref().map(Cow::Borrowed),
            GotoTarget::FunctionDef(function) => Some(Cow::Borrowed(function.name.as_str())),
            GotoTarget::ClassDef(class) => Some(Cow::Borrowed(class.name.as_str())),
            GotoTarget::Parameter(parameter) => Some(Cow::Borrowed(parameter.name.as_str())),
            GotoTarget::ImportSymbolAlias { asname, .. } => Some(Cow::Borrowed(asname.as_str())),
            GotoTarget::ImportExportedName { alias, .. } => {
                Some(Cow::Borrowed(alias.name.as_str()))
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
            GotoTarget::ImportModuleAlias { asname, .. } => Some(Cow::Borrowed(asname.as_str())),
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
        model: &SemanticModel,
        covering_node: &CoveringNode<'a>,
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
                                    return Some(GotoTarget::ImportModuleAlias { alias, asname });
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
                                        level: 0,
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
                                    return Some(GotoTarget::ImportSymbolAlias { alias, asname });
                                }
                            }

                            // Is the offset in the original name part?
                            if alias.name.range.contains_inclusive(offset) {
                                return Some(GotoTarget::ImportExportedName { alias, import_from });
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
                        if let Some((component_index, component_range)) =
                            find_module_component(&full_module_name, module_expr.start(), offset)
                        {
                            return Some(GotoTarget::ImportModuleComponent {
                                module_name: full_module_name,
                                level: from.level,
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

            node @ AnyNodeRef::ExprStringLiteral(string_expr) => {
                // Check if we've clicked on a sub-GotoTarget inside a string annotation's sub-AST
                if let Some((subast, submodel)) = model.enter_string_annotation(string_expr)
                    && let Some(GotoTarget::Expression(subexpr)) = find_goto_target_impl(
                        &submodel,
                        subast.tokens(),
                        subast.syntax().into(),
                        offset,
                    )
                {
                    let name = match subexpr {
                        ast::ExprRef::Name(name) => Some(name.id.to_string()),
                        ast::ExprRef::Attribute(attr) => Some(attr.attr.to_string()),
                        _ => None,
                    };
                    Some(GotoTarget::StringAnnotationSubexpr {
                        string_expr,
                        subrange: subexpr.range(),
                        name,
                    })
                } else {
                    node.as_expr_ref().map(GotoTarget::Expression)
                }
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
            GotoTarget::ImportSymbolAlias { asname, .. } => asname.range,
            Self::ImportExportedName { alias, .. } => alias.name.range,
            GotoTarget::ImportModuleComponent {
                component_range, ..
            } => *component_range,
            GotoTarget::StringAnnotationSubexpr { subrange, .. } => *subrange,
            GotoTarget::ImportModuleAlias { asname, .. } => asname.range,
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
fn convert_resolved_definitions_to_targets<'db>(
    db: &'db dyn ty_python_semantic::Db,
    definitions: Vec<ty_python_semantic::ResolvedDefinition<'db>>,
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
                crate::NavigationTarget::from(file_range)
            }
        })
        .collect()
}

/// Shared helper to get definitions for an expr (that is presumably a name/attr)
fn definitions_for_expression<'db>(
    model: &SemanticModel<'db>,
    expression: ruff_python_ast::ExprRef<'_>,
    alias_resolution: ImportAliasResolution,
) -> Option<Vec<ResolvedDefinition<'db>>> {
    match expression {
        ast::ExprRef::Name(name) => Some(definitions_for_name(
            model,
            name.id.as_str(),
            expression.into(),
            alias_resolution,
        )),
        ast::ExprRef::Attribute(attribute) => Some(ty_python_semantic::definitions_for_attribute(
            model, attribute,
        )),
        _ => None,
    }
}

fn definitions_for_callable<'db>(
    model: &SemanticModel<'db>,
    call: &ast::ExprCall,
) -> Vec<ResolvedDefinition<'db>> {
    // Attempt to refine to a specific call
    let signature_info = call_signature_details(model, call);
    signature_info
        .into_iter()
        .filter_map(|signature| signature.definition.map(ResolvedDefinition::Definition))
        .collect()
}

/// Shared helper to map and convert resolved definitions into navigation targets.
fn definitions_to_navigation_targets<'db>(
    db: &dyn ty_python_semantic::Db,
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

pub(crate) fn find_goto_target<'a>(
    model: &'a SemanticModel,
    parsed: &'a ParsedModuleRef,
    offset: TextSize,
) -> Option<GotoTarget<'a>> {
    find_goto_target_impl(model, parsed.tokens(), parsed.syntax().into(), offset)
}

pub(crate) fn find_goto_target_impl<'a>(
    model: &'a SemanticModel,
    tokens: &'a Tokens,
    syntax: AnyNodeRef<'a>,
    offset: TextSize,
) -> Option<GotoTarget<'a>> {
    let token = tokens
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

    let covering_node = covering_node(syntax, token.range())
        .find_first(|node| {
            node.is_identifier() || node.is_expression() || node.is_stmt_import_from()
        })
        .ok()?;

    GotoTarget::from_covering_node(model, &covering_node, offset, tokens)
}

/// Helper function to resolve a module name and create a navigation target.
fn definitions_for_module<'db>(
    model: &SemanticModel<'db>,
    module: Option<&str>,
    level: u32,
) -> Option<Vec<ResolvedDefinition<'db>>> {
    let module = model.resolve_module(module, level)?;
    let file = module.file(model.db())?;
    Some(vec![ResolvedDefinition::Module(file)])
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
    for (i, component) in full_module_name.split('.').enumerate() {
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

/// Helper to get the module name up to the given component index
fn import_name(module_name: &str, component_index: usize) -> &str {
    // We want everything to the left of the nth `.`
    // If there's no nth `.` then we want the whole thing.
    let idx = module_name
        .match_indices('.')
        .nth(component_index)
        .map(|(idx, _)| idx)
        .unwrap_or(module_name.len());

    &module_name[..idx]
}
