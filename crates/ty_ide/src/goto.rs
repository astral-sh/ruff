pub use crate::goto_declaration::goto_declaration;
pub use crate::goto_definition::goto_definition;
pub use crate::goto_type_definition::goto_type_definition;

use std::borrow::Cow;

use crate::find_node::covering_node;
use crate::stub_mapping::StubMapper;
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::types::Type;
use ty_python_semantic::types::definitions_for_keyword_argument;
use ty_python_semantic::{
    HasType, SemanticModel, definitions_for_imported_symbol, definitions_for_name,
};

#[derive(Clone, Debug)]
pub(crate) enum GotoTarget<'a> {
    Expression(ast::ExprRef<'a>),
    FunctionDef(&'a ast::StmtFunctionDef),
    ClassDef(&'a ast::StmtClassDef),
    Parameter(&'a ast::Parameter),

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
            // TODO: Support identifier targets
            GotoTarget::PatternMatchRest(_)
            | GotoTarget::PatternKeywordArgument(_)
            | GotoTarget::PatternMatchStarName(_)
            | GotoTarget::PatternMatchAsName(_)
            | GotoTarget::ImportModuleComponent { .. }
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
        &self,
        file: ruff_db::files::File,
        db: &dyn crate::Db,
        stub_mapper: Option<&StubMapper>,
    ) -> Option<crate::NavigationTargets> {
        use crate::NavigationTarget;
        use ruff_python_ast as ast;

        match self {
            GotoTarget::Expression(expression) => match expression {
                ast::ExprRef::Name(name) => definitions_to_navigation_targets(
                    db,
                    stub_mapper,
                    definitions_for_name(db, file, name),
                ),
                ast::ExprRef::Attribute(attribute) => definitions_to_navigation_targets(
                    db,
                    stub_mapper,
                    ty_python_semantic::definitions_for_attribute(db, file, attribute),
                ),
                _ => None,
            },

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

            // For import aliases (offset within 'y' or 'z' in "from x import y as z")
            GotoTarget::ImportSymbolAlias {
                alias, import_from, ..
            } => {
                // Handle both original names and alias names in `from x import y as z` statements
                let symbol_name = alias.name.as_str();
                let definitions =
                    definitions_for_imported_symbol(db, file, import_from, symbol_name);

                definitions_to_navigation_targets(db, stub_mapper, definitions)
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
                resolve_module_to_navigation_target(db, &target_module_name)
            }

            // Handle import aliases (offset within 'z' in "import x.y as z")
            GotoTarget::ImportModuleAlias { alias } => {
                // For import aliases, navigate to the module being aliased
                // This only applies to regular import statements like "import x.y as z"
                let full_module_name = alias.name.as_str();

                // Try to resolve the module
                resolve_module_to_navigation_target(db, full_module_name)
            }

            // Handle keyword arguments in call expressions
            GotoTarget::KeywordArgument {
                keyword,
                call_expression,
            } => {
                let definitions =
                    definitions_for_keyword_argument(db, file, keyword, call_expression);
                definitions_to_navigation_targets(db, stub_mapper, definitions)
            }

            // For exception variables, they are their own definitions (like parameters)
            GotoTarget::ExceptVariable(except_handler) => {
                if let Some(name) = &except_handler.name {
                    let range = name.range;
                    Some(crate::NavigationTargets::single(NavigationTarget::new(
                        file, range,
                    )))
                } else {
                    None
                }
            }

            // For pattern match rest variables, they are their own definitions
            GotoTarget::PatternMatchRest(pattern_mapping) => {
                if let Some(rest_name) = &pattern_mapping.rest {
                    let range = rest_name.range;
                    Some(crate::NavigationTargets::single(NavigationTarget::new(
                        file, range,
                    )))
                } else {
                    None
                }
            }

            // For pattern match as names, they are their own definitions
            GotoTarget::PatternMatchAsName(pattern_as) => {
                if let Some(name) = &pattern_as.name {
                    let range = name.range;
                    Some(crate::NavigationTargets::single(NavigationTarget::new(
                        file, range,
                    )))
                } else {
                    None
                }
            }

            // TODO: Handle string literals that map to TypedDict fields
            _ => None,
        }
    }

    /// Returns the text representation of this goto target.
    /// Returns `None` if no meaningful string representation can be provided.
    /// This is used by the "references" feature, which looks for references
    /// to this goto target.
    pub(crate) fn to_string(&self) -> Option<Cow<str>> {
        match self {
            GotoTarget::Expression(expression) => match expression {
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
        }
    }

    /// Creates a `GotoTarget` from a `CoveringNode` and an offset within the node
    pub(crate) fn from_covering_node<'a>(
        covering_node: &crate::find_node::CoveringNode<'a>,
        offset: TextSize,
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
}

impl Ranged for GotoTarget<'_> {
    fn range(&self) -> TextRange {
        match self {
            GotoTarget::Expression(expression) => expression.range(),
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
            ty_python_semantic::ResolvedDefinition::FileWithRange(file_range) => {
                // For file ranges, navigate to the specific range within the file
                crate::NavigationTarget::new(file_range.file(), file_range.range())
            }
        })
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
            _ => 0,
        })?;

    let covering_node = covering_node(parsed.syntax().into(), token.range())
        .find_first(|node| node.is_identifier() || node.is_expression())
        .ok()?;

    GotoTarget::from_covering_node(&covering_node, offset)
}

/// Helper function to resolve a module name and create a navigation target.
fn resolve_module_to_navigation_target(
    db: &dyn crate::Db,
    module_name_str: &str,
) -> Option<crate::NavigationTargets> {
    use ty_python_semantic::{ModuleName, resolve_module};

    if let Some(module_name) = ModuleName::new(module_name_str) {
        if let Some(resolved_module) = resolve_module(db, &module_name) {
            if let Some(module_file) = resolved_module.file() {
                return Some(crate::NavigationTargets::single(
                    crate::NavigationTarget::new(module_file, TextRange::default()),
                ));
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
