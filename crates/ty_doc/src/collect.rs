use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;

use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_formatter::{PyFormatOptions, format_module_source};
use ruff_source_file::{LineIndex, SourceCode};
use ruff_text_size::{Ranged, TextRange};
use ty_ide::exported_symbols;
use ty_module_resolver::{ModuleName, file_to_module, resolve_module};
use ty_project::{Db as _, ProjectDatabase};
use ty_python_core::definition::{
    AnnotatedAssignmentDefinitionKind, AssignmentDefinitionKind, Definition, DefinitionKind,
};
use ty_python_semantic::types::TypeDefinition;
use ty_python_semantic::types::ide_support::{
    function_overload_details, property_getter_definitions_for_function,
};
use ty_python_semantic::{
    HasType, ImportAliasResolution, ResolvedDefinition, SemanticModel, definitions_for_name,
    end_of_scope_class_members, end_of_scope_module_members, type_hierarchy_supertypes,
};

use crate::model::{
    ClassBaseDoc, ClassDoc, Documentation, ExtractedModule, FunctionDoc, FunctionSignatureDoc,
    ModuleDoc, SourceDoc, VariableDoc, VariableKind, build_type_index, module_short_name,
    parent_module, parent_modules, sanitize_path_segment,
};
use crate::syntax::{dotted_name_run_end, parse_python_tokens};

impl Documentation {
    pub(crate) fn collect(
        db: &ProjectDatabase,
        document_private_items: bool,
        default_selection: bool,
        generator_version: String,
    ) -> Self {
        let project = db.project();
        let project_name = project.name(db).to_string();
        let project_slug = sanitize_path_segment(&project_name);
        let files = project.files(db).into_iter().collect::<Vec<_>>();
        let default_package_root = default_selection
            .then(|| default_package_root(db, &files, &project_name))
            .flatten();
        let files = files
            .into_iter()
            .filter(|file| {
                !default_selection
                    || is_documented_by_default(db, *file, default_package_root.as_deref())
            })
            .collect::<Vec<_>>();
        let documented_files = files.len();

        let modules = Mutex::new(Vec::with_capacity(documented_files));
        let warnings = Mutex::new(Vec::new());

        {
            let modules = &modules;
            let warnings = &warnings;

            let db = db.clone();
            rayon::scope(move |scope| {
                for file in files {
                    let db = db.clone();
                    scope.spawn(move |_| {
                        let extracted = extract_module(&db, file, document_private_items);
                        warnings.lock().unwrap().extend(extracted.warnings);
                        if let Some(module) = extracted.module {
                            modules.lock().unwrap().push(module);
                        }
                    });
                }
            });
        }

        let mut warnings = warnings.into_inner().unwrap();
        warnings.sort_by(|left, right| {
            left.rendering_sort_key(db)
                .cmp(&right.rendering_sort_key(db))
        });

        let mut modules_by_name = BTreeMap::new();
        for module in modules.into_inner().unwrap() {
            match modules_by_name.entry(module.name.clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(module);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if module_precedes_existing(entry.get(), &module) {
                        entry.insert(module);
                    }
                }
            }
        }

        let module_names: Vec<String> = modules_by_name.keys().cloned().collect();
        for module_name in &module_names {
            for parent in parent_modules(module_name) {
                modules_by_name
                    .entry(parent.clone())
                    .or_insert_with(|| ModuleDoc::synthetic(parent));
            }
        }

        for module_name in module_names {
            if let Some(parent) = parent_module(&module_name)
                && let Some(parent_doc) = modules_by_name.get_mut(parent)
            {
                parent_doc.submodules.insert(module_name);
            }
        }

        let type_index = build_type_index(&modules_by_name);

        Self {
            project_name,
            project_slug,
            generator_version,
            modules: modules_by_name,
            type_index,
            warnings,
            documented_files,
        }
    }
}

fn module_precedes_existing(existing: &ModuleDoc, candidate: &ModuleDoc) -> bool {
    documented_module_precedence(candidate) > documented_module_precedence(existing)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum DocumentedModulePrecedence {
    Unknown,
    RuntimeModule,
    StubModule,
    RuntimePackage,
    StubPackage,
}

fn documented_module_precedence(module: &ModuleDoc) -> (DocumentedModulePrecedence, &str) {
    let path = module
        .source
        .as_ref()
        .map_or("", |source| source.path.as_str());
    let path_ref = Path::new(path);
    let file_name = path_ref.file_name().and_then(|name| name.to_str());
    let extension = path_ref
        .extension()
        .and_then(|extension| extension.to_str());

    let precedence = if file_name.is_some_and(|name| name.eq_ignore_ascii_case("__init__.pyi")) {
        DocumentedModulePrecedence::StubPackage
    } else if file_name.is_some_and(|name| name.eq_ignore_ascii_case("__init__.py")) {
        DocumentedModulePrecedence::RuntimePackage
    } else if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("pyi")) {
        DocumentedModulePrecedence::StubModule
    } else if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("py")) {
        DocumentedModulePrecedence::RuntimeModule
    } else {
        DocumentedModulePrecedence::Unknown
    };

    (precedence, path)
}

pub(crate) fn collect_signature_links(
    model: &SemanticModel<'_>,
    scope_node: AnyNodeRef<'_>,
    signature: &str,
) -> BTreeMap<String, String> {
    let mut links = BTreeMap::new();

    for path in signature_identifier_paths(signature) {
        if path.contains('.') {
            collect_qualified_stdlib_links(model, scope_node, &path, &mut links);
        } else if let Some(href) = stdlib_link_for_name(model, scope_node, &path) {
            links.insert(path, href);
        }
    }

    links
}

pub(crate) fn collect_function_annotation_links(
    model: &SemanticModel,
    function: &ruff_python_ast::StmtFunctionDef,
    links: &mut BTreeMap<String, String>,
) {
    for parameter in &function.parameters {
        if let Some(annotation) = parameter.annotation() {
            collect_expression_type_links(model, annotation, links);
        }
    }

    if let Some(returns) = function.returns.as_deref() {
        collect_expression_type_links(model, returns, links);
    }
}

pub(crate) fn collect_class_argument_links(
    model: &SemanticModel,
    class: &ruff_python_ast::StmtClassDef,
    links: &mut BTreeMap<String, String>,
) {
    let Some(arguments) = class.arguments.as_deref() else {
        return;
    };

    for argument in &arguments.args {
        collect_expression_type_links(model, argument, links);
    }
    for keyword in &arguments.keywords {
        collect_expression_type_links(model, &keyword.value, links);
    }
}

pub(crate) fn collect_expression_type_links(
    model: &SemanticModel,
    expression: &ruff_python_ast::Expr,
    links: &mut BTreeMap<String, String>,
) {
    ExpressionTypeLinkCollector { model, links }.visit_expr(expression);
}

struct ExpressionTypeLinkCollector<'a, 'db> {
    model: &'a SemanticModel<'db>,
    links: &'a mut BTreeMap<String, String>,
}

impl SourceOrderVisitor<'_> for ExpressionTypeLinkCollector<'_, '_> {
    fn visit_expr(&mut self, expression: &ruff_python_ast::Expr) {
        match expression {
            ruff_python_ast::Expr::Name(name) => {
                if let Some(ty) = name.inferred_type(self.model)
                    && let Some(href) = stdlib_link_for_type(self.model.db(), ty, name.id.as_str())
                {
                    self.links.insert(name.id.to_string(), href);
                }
            }
            ruff_python_ast::Expr::Attribute(attribute) => {
                if let Some(ty) = attribute.inferred_type(self.model)
                    && let Some(href) =
                        stdlib_link_for_type(self.model.db(), ty, attribute.attr.as_str())
                {
                    self.links.insert(attribute.attr.to_string(), href);
                }
            }
            _ => {}
        }
        source_order::walk_expr(self, expression);
    }
}

fn signature_identifier_paths(signature: &str) -> BTreeSet<String> {
    let tokens = parse_python_tokens(signature)
        .iter()
        .copied()
        .take_while(|token| !token.kind().is_eof())
        .collect::<Vec<_>>();
    let mut paths = BTreeSet::new();
    let mut index = 0_usize;

    while let Some(token) = tokens.get(index).copied() {
        if token.kind() != TokenKind::Name {
            index += 1;
            continue;
        }

        if let Some(run_end) = dotted_name_run_end(&tokens, index) {
            let last = tokens[run_end - 1];
            paths.insert(signature[TextRange::new(token.start(), last.end())].to_string());
            index = run_end;
        } else {
            paths.insert(signature[token.range()].to_string());
            index += 1;
        }
    }

    paths
}

fn collect_qualified_stdlib_links(
    model: &SemanticModel<'_>,
    scope_node: AnyNodeRef<'_>,
    path: &str,
    links: &mut BTreeMap<String, String>,
) {
    let mut components = path.split('.');
    let Some(first) = components.next() else {
        return;
    };

    let Some(mut module_name) = stdlib_module_for_name(model, scope_node, first) else {
        return;
    };

    let mut display_path = first.to_string();
    links.insert(display_path.clone(), python_stdlib_href(&module_name, None));

    for component in components {
        display_path.push('.');
        display_path.push_str(component);

        let mut candidate = module_name.clone();
        candidate.push('.');
        candidate.push_str(component);

        if is_stdlib_module(model, &candidate) {
            module_name = candidate;
            links.insert(display_path.clone(), python_stdlib_href(&module_name, None));
        } else {
            links.insert(
                display_path.clone(),
                python_stdlib_href(&module_name, Some(component)),
            );
        }
    }
}

fn stdlib_module_for_name(
    model: &SemanticModel<'_>,
    scope_node: AnyNodeRef<'_>,
    name: &str,
) -> Option<String> {
    definitions_for_name(
        model,
        name,
        scope_node,
        ImportAliasResolution::ResolveAliases,
    )
    .into_iter()
    .find_map(|definition| stdlib_module_for_definition(model.db(), &definition))
}

fn stdlib_module_for_definition<'db>(
    db: &'db dyn ty_python_semantic::Db,
    definition: &ResolvedDefinition<'db>,
) -> Option<String> {
    let file = match definition {
        ResolvedDefinition::Definition(definition) => (*definition).file(db),
        ResolvedDefinition::Module(file) => *file,
        ResolvedDefinition::FileWithRange(range) => range.file(),
    };
    stdlib_module_name(db, file)
}

fn stdlib_link_for_type<'db>(
    db: &'db dyn ty_python_semantic::Db,
    ty: ty_python_semantic::types::Type<'db>,
    display_name: &str,
) -> Option<String> {
    let definition = ty.definition(db)?;
    let (file, symbol) = match definition {
        TypeDefinition::Module(module) => {
            return module
                .search_path(db)
                .is_some_and(ty_module_resolver::SearchPath::is_standard_library)
                .then(|| python_stdlib_href(module.name(db).as_str(), None));
        }
        TypeDefinition::StaticClass(definition)
        | TypeDefinition::DynamicClass(definition)
        | TypeDefinition::Function(definition)
        | TypeDefinition::TypeVar(definition)
        | TypeDefinition::TypeAlias(definition)
        | TypeDefinition::NewType(definition)
        | TypeDefinition::SpecialForm(definition)
        | TypeDefinition::EnumMember(definition) => (
            definition.file(db),
            definition
                .name(db)
                .unwrap_or_else(|| display_name.to_string()),
        ),
    };

    stdlib_href_for_file(db, file, Some(&symbol))
}

fn stdlib_module_name(db: &dyn ty_python_semantic::Db, file: File) -> Option<String> {
    let module = file_to_module(db, file)?;
    let module_name = module.name(db).as_str();
    (module_name != "ty_extensions"
        && module
            .search_path(db)
            .is_some_and(ty_module_resolver::SearchPath::is_standard_library))
    .then(|| module_name.to_string())
}

fn stdlib_href_for_file(
    db: &dyn ty_python_semantic::Db,
    file: File,
    symbol: Option<&str>,
) -> Option<String> {
    let module_name = stdlib_module_name(db, file)?;
    Some(python_stdlib_href(
        &module_name,
        symbol.filter(|symbol| module_short_name(&module_name) != *symbol),
    ))
}

fn is_stdlib_module(model: &SemanticModel<'_>, module_name: &str) -> bool {
    let Some(module_name) = ModuleName::new(module_name) else {
        return false;
    };
    resolve_module(model.db(), model.file(), &module_name).is_some_and(|module| {
        module
            .search_path(model.db())
            .is_some_and(ty_module_resolver::SearchPath::is_standard_library)
    })
}

fn stdlib_link_for_name(
    model: &SemanticModel<'_>,
    scope_node: AnyNodeRef<'_>,
    name: &str,
) -> Option<String> {
    definitions_for_name(
        model,
        name,
        scope_node,
        ImportAliasResolution::ResolveAliases,
    )
    .into_iter()
    .find_map(|definition| stdlib_link_for_definition(model.db(), &definition, name))
}

fn stdlib_link_for_definition<'db>(
    db: &'db dyn ty_python_semantic::Db,
    definition: &ResolvedDefinition<'db>,
    name: &str,
) -> Option<String> {
    let (file, symbol) = match definition {
        ResolvedDefinition::Definition(definition) => (
            (*definition).file(db),
            Some(definition.name(db).unwrap_or_else(|| name.to_string())),
        ),
        ResolvedDefinition::Module(file) => (*file, None),
        ResolvedDefinition::FileWithRange(range) => (range.file(), Some(name.to_string())),
    };
    stdlib_href_for_file(db, file, symbol.as_deref())
}

fn python_stdlib_href(module: &str, symbol: Option<&str>) -> String {
    let page = if module == "builtins" {
        "functions"
    } else {
        module
    };
    let mut href = format!("https://docs.python.org/3/library/{page}.html");

    if let Some(symbol) = symbol {
        href.push('#');
        if module == "builtins" {
            href.push_str(symbol);
        } else {
            href.push_str(module);
            href.push('.');
            href.push_str(symbol);
        }
    }

    href
}

pub(crate) fn extract_module(
    db: &ProjectDatabase,
    file: File,
    document_private_items: bool,
) -> ExtractedModule {
    let module_name = module_name_for_file(db, file);

    if !document_private_items && is_private_module(&module_name) {
        return ExtractedModule {
            module: None,
            warnings: Vec::new(),
        };
    }

    let source = source_text(db, file);
    let parsed = parsed_module(db, file).load(db);
    let line_index = line_index(db, file);
    let source_code = SourceCode::new(source.as_str(), &line_index);

    let mut warnings = Vec::new();
    if let Some(read_error) = source.read_error() {
        let mut diagnostic = Diagnostic::new(DiagnosticId::Io, Severity::Warning, read_error);
        diagnostic.annotate(Annotation::primary(Span::from(file)));
        warnings.push(diagnostic);
    }

    for error in parsed.errors() {
        let mut diagnostic =
            Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Warning, &error.error);
        diagnostic.annotate(Annotation::primary(
            Span::from(file).with_range(error.range()),
        ));
        warnings.push(diagnostic);
    }

    for error in parsed.unsupported_syntax_errors() {
        let mut diagnostic = Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Warning, error);
        diagnostic.annotate(Annotation::primary(
            Span::from(file).with_range(error.range()),
        ));
        warnings.push(diagnostic);
    }

    let body = &parsed.syntax().body;
    let semantic_model = SemanticModel::new(db, file);
    let source_path = relative_file_path(db, file);
    let mut module = ModuleDoc {
        name: module_name,
        docstring: docstring_from_body(body),
        source: Some(SourceDoc {
            path: source_path,
            text: source.as_str().to_string(),
            tokens: parsed.tokens().clone(),
        }),
        submodules: BTreeSet::new(),
        public_items: exported_symbols(db, file)
            .iter()
            .map(|(_, symbol)| symbol.name.to_string())
            .collect(),
        classes: Vec::new(),
        functions: Vec::new(),
        variables: Vec::new(),
    };
    for member in end_of_scope_module_members(db, file) {
        if !document_private_items && !is_public_name(member.member.name.as_str()) {
            continue;
        }

        let definition = member.first_reachable_definition;
        match definition.kind(db) {
            DefinitionKind::AnnotatedAssignment(assign) => {
                if let Some(variable) = extract_annotated_assignment_definition(
                    db,
                    &parsed,
                    &source_code,
                    &line_index,
                    definition,
                    assign,
                    &semantic_model,
                ) {
                    module.variables.push(variable);
                }
            }
            DefinitionKind::Assignment(assign) => {
                if let Some(variable) = extract_assignment_definition(
                    db,
                    &parsed,
                    &source_code,
                    &line_index,
                    definition,
                    assign,
                ) {
                    module.variables.push(variable);
                }
            }
            DefinitionKind::TypeAlias(type_alias) => {
                if let Some(variable) = extract_type_alias_definition(
                    db,
                    &source_code,
                    &line_index,
                    definition,
                    type_alias.node(&parsed),
                    &semantic_model,
                ) {
                    module.variables.push(variable);
                }
            }
            DefinitionKind::Function(function) => {
                module.functions.push(extract_function_group(
                    &parsed,
                    &source_code,
                    &line_index,
                    function.node(&parsed),
                    &semantic_model,
                ));
            }
            DefinitionKind::Class(class) => {
                module.classes.push(extract_class(
                    db,
                    &source_code,
                    &line_index,
                    class.node(&parsed),
                    &semantic_model,
                    document_private_items,
                ));
            }
            _ => {}
        }
    }

    sort_module_items(&mut module);

    ExtractedModule {
        module: Some(module),
        warnings,
    }
}

fn extract_function_group(
    parsed: &ruff_db::parsed::ParsedModuleRef,
    source_code: &SourceCode,
    line_index: &LineIndex,
    function: &ruff_python_ast::StmtFunctionDef,
    semantic_model: &SemanticModel,
) -> FunctionDoc {
    let overload_details = function_overload_details(semantic_model, function);
    let mut overloads = overload_details
        .overloads
        .iter()
        .filter_map(|definition| {
            extract_resolved_function(parsed, source_code, line_index, semantic_model, definition)
        })
        .collect::<Vec<_>>();

    if let Some(mut implementation) =
        overload_details
            .implementation
            .as_ref()
            .and_then(|definition| {
                extract_resolved_function(
                    parsed,
                    source_code,
                    line_index,
                    semantic_model,
                    definition,
                )
            })
    {
        implementation.overloads = overloads
            .iter()
            .map(FunctionSignatureDoc::from_function)
            .collect();
        return implementation;
    }

    if overloads.is_empty() {
        return extract_function(source_code, line_index, function, semantic_model);
    }

    let mut function = overloads.remove(0);
    function.overload_only = true;
    function.overloads = std::iter::once(FunctionSignatureDoc::from_function(&function))
        .chain(overloads.iter().map(FunctionSignatureDoc::from_function))
        .collect();
    function
}

fn extract_resolved_function(
    parsed: &ruff_db::parsed::ParsedModuleRef,
    source_code: &SourceCode,
    line_index: &LineIndex,
    semantic_model: &SemanticModel,
    definition: &ResolvedDefinition,
) -> Option<FunctionDoc> {
    let definition = definition.definition()?;
    let DefinitionKind::Function(function) = definition.kind(semantic_model.db()) else {
        return None;
    };

    Some(extract_function(
        source_code,
        line_index,
        function.node(parsed),
        semantic_model,
    ))
}

fn extract_function(
    source_code: &SourceCode,
    line_index: &LineIndex,
    function: &ruff_python_ast::StmtFunctionDef,
    semantic_model: &SemanticModel,
) -> FunctionDoc {
    let signature = header_signature(
        source_code,
        function.range,
        &function.body,
        HeaderKind::Function,
    );
    let mut signature_links = collect_signature_links(semantic_model, function.into(), &signature);
    collect_function_annotation_links(semantic_model, function, &mut signature_links);

    FunctionDoc {
        name: function.name.to_string(),
        signature_links,
        signature,
        docstring: docstring_from_body(&function.body),
        source_line: line_number(line_index, &function.name),
        overloads: Vec::new(),
        overload_only: false,
    }
}

fn extract_property(
    source_code: &SourceCode,
    line_index: &LineIndex,
    function: &ruff_python_ast::StmtFunctionDef,
    semantic_model: &SemanticModel,
) -> VariableDoc {
    let (signature, signature_links) = function.returns.as_deref().map_or_else(
        || (String::new(), BTreeMap::new()),
        |returns| {
            let signature = annotated_variable_signature(source_code, returns.range());
            let mut signature_links =
                collect_signature_links(semantic_model, returns.into(), &signature);
            collect_expression_type_links(semantic_model, returns, &mut signature_links);
            (signature, signature_links)
        },
    );

    VariableDoc {
        name: function.name.to_string(),
        signature_links,
        signature,
        docstring: docstring_from_body(&function.body),
        source_line: line_number(line_index, &function.name),
        kind: VariableKind::Variable,
    }
}

fn is_property_function(
    function: &ruff_python_ast::StmtFunctionDef,
    semantic_model: &SemanticModel,
) -> bool {
    function.decorator_list.iter().any(|decorator| {
        is_property_decorator(&decorator.expression, function.into(), semantic_model)
    })
}

fn is_property_decorator(
    expression: &ruff_python_ast::Expr,
    scope_node: AnyNodeRef<'_>,
    semantic_model: &SemanticModel,
) -> bool {
    let Some(path) = dotted_expression_path(expression) else {
        return false;
    };

    match path.as_slice() {
        [name] => module_for_name(semantic_model, scope_node, name)
            .is_some_and(|module| is_property_decorator_target(&module, name)),
        [first, components @ ..] => {
            let Some(name) = components.last() else {
                return false;
            };
            module_for_name(semantic_model, scope_node, first)
                .is_some_and(|module| is_property_decorator_target(&module, name))
        }
        [] => false,
    }
}

fn is_property_decorator_target(module: &str, name: &str) -> bool {
    (module == "builtins" && name == "property")
        || (module == "functools" && name == "cached_property")
}

fn is_property_modifier_function(function: &ruff_python_ast::StmtFunctionDef) -> bool {
    function.decorator_list.iter().any(|decorator| {
        dotted_expression_path(&decorator.expression)
            .is_some_and(|path| matches!(path.as_slice(), [_, "setter" | "deleter"]))
    })
}

fn dotted_expression_path(expression: &ruff_python_ast::Expr) -> Option<Vec<&str>> {
    let mut path = Vec::new();
    collect_dotted_expression_path(expression, &mut path).then_some(path)
}

fn collect_dotted_expression_path<'a>(
    expression: &'a ruff_python_ast::Expr,
    path: &mut Vec<&'a str>,
) -> bool {
    match expression {
        ruff_python_ast::Expr::Name(name) => {
            path.push(name.id.as_str());
            true
        }
        ruff_python_ast::Expr::Attribute(attribute) => {
            if collect_dotted_expression_path(&attribute.value, path) {
                path.push(attribute.attr.as_str());
                true
            } else {
                false
            }
        }
        ruff_python_ast::Expr::Call(call) => collect_dotted_expression_path(&call.func, path),
        _ => false,
    }
}

fn module_for_name(
    model: &SemanticModel<'_>,
    scope_node: AnyNodeRef<'_>,
    name: &str,
) -> Option<String> {
    definitions_for_name(
        model,
        name,
        scope_node,
        ImportAliasResolution::ResolveAliases,
    )
    .into_iter()
    .find_map(|definition| module_for_definition(model.db(), &definition))
}

fn module_for_definition<'db>(
    db: &'db dyn ty_python_semantic::Db,
    definition: &ResolvedDefinition<'db>,
) -> Option<String> {
    let file = match definition {
        ResolvedDefinition::Definition(definition) => (*definition).file(db),
        ResolvedDefinition::Module(file) => *file,
        ResolvedDefinition::FileWithRange(range) => range.file(),
    };
    Some(file_to_module(db, file)?.name(db).as_str().to_string())
}

fn extract_class(
    db: &ProjectDatabase,
    source_code: &SourceCode,
    line_index: &LineIndex,
    class: &ruff_python_ast::StmtClassDef,
    semantic_model: &SemanticModel,
    document_private_items: bool,
) -> ClassDoc {
    let mut methods = Vec::new();
    let mut attributes = Vec::new();
    let parsed = parsed_module(db, semantic_model.file()).load(db);
    for member in end_of_scope_class_members(semantic_model, class) {
        if !document_private_items && !is_public_name(member.member.name.as_str()) {
            continue;
        }

        let definition = member.first_reachable_definition;
        match definition.kind(db) {
            DefinitionKind::AnnotatedAssignment(assign) => {
                if let Some(attribute) = extract_annotated_assignment_definition(
                    db,
                    &parsed,
                    source_code,
                    line_index,
                    definition,
                    assign,
                    semantic_model,
                ) {
                    attributes.push(attribute);
                }
            }
            DefinitionKind::Assignment(assign) => {
                if let Some(attribute) = extract_assignment_definition(
                    db,
                    &parsed,
                    source_code,
                    line_index,
                    definition,
                    assign,
                ) {
                    attributes.push(attribute);
                }
            }
            DefinitionKind::Function(function) => {
                let function = function.node(&parsed);
                if let Some(getter) = property_getter_definitions_for_function(
                    semantic_model,
                    function,
                    ImportAliasResolution::ResolveAliases,
                )
                .into_iter()
                .filter_map(|definition| definition.definition())
                .find_map(|definition| {
                    let DefinitionKind::Function(function) = definition.kind(db) else {
                        return None;
                    };
                    Some(function.node(&parsed))
                }) {
                    attributes.push(extract_property(
                        source_code,
                        line_index,
                        getter,
                        semantic_model,
                    ));
                } else if is_property_function(function, semantic_model) {
                    attributes.push(extract_property(
                        source_code,
                        line_index,
                        function,
                        semantic_model,
                    ));
                } else if !is_property_modifier_function(function) {
                    methods.push(extract_function_group(
                        &parsed,
                        source_code,
                        line_index,
                        function,
                        semantic_model,
                    ));
                }
            }
            _ => {}
        }
    }

    let signature = header_signature(source_code, class.range, &class.body, HeaderKind::Class);
    let mut signature_links = collect_signature_links(semantic_model, class.into(), &signature);
    collect_class_argument_links(semantic_model, class, &mut signature_links);
    let enum_member_names = class
        .inferred_type(semantic_model)
        .and_then(|ty| ty.enum_member_names(db))
        .unwrap_or_default()
        .into_iter()
        .collect();

    sort_class_items(&mut methods, &mut attributes);

    ClassDoc {
        name: class.name.to_string(),
        signature_links,
        signature,
        base_classes: extract_base_classes(db, class, semantic_model),
        enum_member_names,
        docstring: docstring_from_body(&class.body),
        source_line: line_number(line_index, &class.name),
        methods,
        attributes,
    }
}

fn sort_module_items(module: &mut ModuleDoc) {
    module
        .classes
        .sort_by(|left, right| left.name.cmp(&right.name));
    module
        .functions
        .sort_by(|left, right| left.name.cmp(&right.name));
    module.variables.sort_by(|left, right| {
        variable_kind_order(left.kind)
            .cmp(&variable_kind_order(right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn sort_class_items(methods: &mut [FunctionDoc], attributes: &mut [VariableDoc]) {
    methods.sort_by(|left, right| left.name.cmp(&right.name));
    attributes.sort_by(|left, right| {
        variable_kind_order(left.kind)
            .cmp(&variable_kind_order(right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });
}

const fn variable_kind_order(kind: VariableKind) -> u8 {
    match kind {
        VariableKind::Variable => 0,
        VariableKind::TypeAlias => 1,
    }
}

fn extract_base_classes(
    db: &ProjectDatabase,
    class: &ruff_python_ast::StmtClassDef,
    semantic_model: &SemanticModel,
) -> Vec<ClassBaseDoc> {
    if class
        .arguments
        .as_deref()
        .is_none_or(|arguments| arguments.args.is_empty())
    {
        return Vec::new();
    }

    let Some(class_type) = class.inferred_type(semantic_model) else {
        return Vec::new();
    };

    type_hierarchy_supertypes(db, class_type)
        .into_iter()
        .filter_map(|base| {
            let module = file_to_module(db, base.file)?;
            Some(ClassBaseDoc {
                module: module.name(db).to_string(),
                name: base.name.to_string(),
            })
        })
        .collect()
}

fn extract_annotated_assignment_definition(
    db: &ProjectDatabase,
    parsed: &ruff_db::parsed::ParsedModuleRef,
    source_code: &SourceCode,
    line_index: &LineIndex,
    definition: Definition,
    assign: &AnnotatedAssignmentDefinitionKind,
    semantic_model: &SemanticModel,
) -> Option<VariableDoc> {
    let annotation = assign.annotation(parsed);
    let name = definition.name(db)?;

    let signature = append_constant_default(
        annotated_variable_signature(source_code, annotation.range()),
        source_code,
        assign.value(parsed),
    );
    let mut signature_links =
        collect_signature_links(semantic_model, annotation.into(), &signature);
    collect_expression_type_links(semantic_model, annotation, &mut signature_links);

    Some(VariableDoc {
        name,
        signature_links,
        signature,
        docstring: definition.docstring(db),
        source_line: line_number_from_range(line_index, definition.full_range(db, parsed).range()),
        kind: VariableKind::Variable,
    })
}

fn extract_assignment_definition(
    db: &ProjectDatabase,
    parsed: &ruff_db::parsed::ParsedModuleRef,
    source_code: &SourceCode,
    line_index: &LineIndex,
    definition: Definition,
    assign: &AssignmentDefinitionKind,
) -> Option<VariableDoc> {
    let value = assign.value(parsed);
    let name = definition.name(db)?;

    let signature = append_constant_default(String::new(), source_code, Some(value));
    Some(VariableDoc {
        name,
        signature_links: BTreeMap::new(),
        signature,
        docstring: definition.docstring(db),
        source_line: line_number_from_range(line_index, definition.full_range(db, parsed).range()),
        kind: VariableKind::Variable,
    })
}

fn extract_type_alias_definition(
    db: &ProjectDatabase,
    source_code: &SourceCode,
    line_index: &LineIndex,
    definition: Definition,
    type_alias: &ruff_python_ast::StmtTypeAlias,
    semantic_model: &SemanticModel,
) -> Option<VariableDoc> {
    let name = definition.name(db)?;

    let signature = type_alias_signature(source_code, type_alias.value.range());
    let mut signature_links =
        collect_signature_links(semantic_model, type_alias.value.as_ref().into(), &signature);
    collect_expression_type_links(semantic_model, &type_alias.value, &mut signature_links);

    Some(VariableDoc {
        name,
        signature_links,
        signature,
        docstring: definition.docstring(db),
        source_line: line_number(line_index, type_alias),
        kind: VariableKind::TypeAlias,
    })
}

#[derive(Copy, Clone)]
enum HeaderKind {
    Function,
    Class,
}

fn header_signature(
    source_code: &SourceCode,
    range: TextRange,
    body: &[ruff_python_ast::Stmt],
    kind: HeaderKind,
) -> String {
    let end = body.first().map_or(range.end(), Ranged::start);
    let header = source_code.slice(TextRange::new(range.start(), end));
    let is_header_line = |line: &str| match kind {
        HeaderKind::Function => {
            let trimmed = line.trim_start();
            trimmed.starts_with("def ") || trimmed.starts_with("async def ")
        }
        HeaderKind::Class => line.trim_start().starts_with("class "),
    };

    let mut lines = header
        .lines()
        .skip_while(|line| !is_header_line(line))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(header.trim());
    }

    let mut signature = lines.join("\n").trim().to_string();
    if let Some(stripped) = signature.strip_suffix(':') {
        signature = stripped.trim_end().to_string();
    }
    format_header_signature(&signature).unwrap_or(signature)
}

fn format_header_signature(signature: &str) -> Option<String> {
    if !signature.contains('\n') && signature.chars().count() <= 88 {
        return None;
    }

    let source = format!("{signature}:\n    ...\n");
    let formatted = format_module_source(&source, PyFormatOptions::default())
        .ok()?
        .into_code();
    let mut header = formatted
        .lines()
        .take_while(|line| line.trim() != "...")
        .collect::<Vec<_>>()
        .join("\n");

    if let Some(stripped) = header.strip_suffix(": ...") {
        header = stripped.trim_end().to_string();
    } else if let Some(stripped) = header.strip_suffix(':') {
        header = stripped.trim_end().to_string();
    }

    Some(header)
}

fn annotated_variable_signature(source_code: &SourceCode, annotation: TextRange) -> String {
    format!(": {}", source_code.slice(annotation).trim())
}

fn append_constant_default(
    signature: String,
    source_code: &SourceCode,
    value: Option<&ruff_python_ast::Expr>,
) -> String {
    let Some(default) = value.and_then(|value| compact_literal_default(source_code, value)) else {
        return signature;
    };

    format!("{signature} = {default}")
}

fn compact_literal_default(
    source_code: &SourceCode,
    value: &ruff_python_ast::Expr,
) -> Option<String> {
    if !matches!(
        value,
        ruff_python_ast::Expr::StringLiteral(_)
            | ruff_python_ast::Expr::BytesLiteral(_)
            | ruff_python_ast::Expr::NumberLiteral(_)
            | ruff_python_ast::Expr::BooleanLiteral(_)
            | ruff_python_ast::Expr::NoneLiteral(_)
    ) {
        return None;
    }

    let expression = expression_signature(source_code, value.range());
    (!expression.contains('\n') && expression.chars().count() <= 80).then_some(expression)
}

fn type_alias_signature(source_code: &SourceCode, value: TextRange) -> String {
    format!(" = {}", expression_signature(source_code, value))
}

fn expression_signature(source_code: &SourceCode, range: TextRange) -> String {
    let expression = source_code.slice(range).trim();
    if expression.contains('\n') {
        compact_multiline_expression_signature(expression)
    } else {
        expression.to_string()
    }
}

fn compact_multiline_expression_signature(expression: &str) -> String {
    let first_line = expression
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or_default();
    let Some(opener) = first_line.chars().last() else {
        return String::new();
    };

    match opener {
        '(' | '[' | '{' => {
            let closer = match opener {
                '(' => ')',
                '[' => ']',
                '{' => '}',
                _ => return format!("{first_line} ..."),
            };
            let compact_delimiter = is_compact_delimiter(first_line, opener);
            if compact_delimiter {
                format!("{first_line}...{closer}")
            } else {
                format!("{first_line} ... {closer}")
            }
        }
        _ => format!("{first_line} ..."),
    }
}

fn is_compact_delimiter(line: &str, delimiter: char) -> bool {
    let Some(prefix) = line.strip_suffix(delimiter) else {
        return false;
    };

    prefix.trim_end().chars().last().is_some_and(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | ']' | ')')
    })
}

fn line_number(line_index: &LineIndex, ranged: impl Ranged) -> String {
    line_index.line_index(ranged.range().start()).to_string()
}

fn line_number_from_range(line_index: &LineIndex, range: TextRange) -> String {
    line_index.line_index(range.start()).to_string()
}

fn docstring_from_body(body: &[ruff_python_ast::Stmt]) -> Option<String> {
    let stmt = body.first()?;
    let expr = stmt.as_expr_stmt()?;
    let literal = expr.value.as_string_literal_expr()?;
    Some(literal.value.to_str().to_string())
}

fn module_name_for_file(db: &ProjectDatabase, file: File) -> String {
    if let Some(module) = file_to_module(db, file) {
        return module.name(db).to_string();
    }

    fallback_module_name(db, file)
}

fn fallback_module_name(db: &ProjectDatabase, file: File) -> String {
    let path = file.path(db);
    let Some(path) = path.as_system_path() else {
        return sanitize_identifier(path.as_ref());
    };

    let root = db.project().root(db);
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut components = relative
        .as_str()
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    if let Some(last) = components.last_mut() {
        for extension in [".pyi", ".py", ".ipynb"] {
            if let Some(stripped) = last.strip_suffix(extension) {
                *last = stripped.to_string();
                break;
            }
        }
    }

    if components
        .last()
        .is_some_and(|component| component == "__init__")
    {
        components.pop();
    }

    let components = components
        .into_iter()
        .map(|component| sanitize_identifier(&component))
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();

    if components.is_empty() {
        sanitize_identifier(db.project().name(db))
    } else {
        components.join(".")
    }
}

fn relative_file_path(db: &ProjectDatabase, file: File) -> String {
    let path = file.path(db);
    let Some(path) = path.as_system_path() else {
        return path.to_string();
    };

    path.strip_prefix(db.project().root(db))
        .map_or_else(|_| path.to_string(), ToString::to_string)
}

pub(crate) fn default_package_root(
    db: &ProjectDatabase,
    files: &[File],
    project_name: &str,
) -> Option<String> {
    let import_root = sanitize_identifier(&project_name.replace('-', "_"));
    files
        .iter()
        .any(|file| path_has_package_root(db, *file, &import_root))
        .then_some(import_root)
}

fn sanitize_identifier(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for (index, character) in value.chars().enumerate() {
        if character == '_'
            || character.is_ascii_alphabetic()
            || (index > 0 && character.is_ascii_digit())
        {
            output.push(character);
        } else {
            output.push('_');
        }
    }
    output.trim_matches('_').to_string()
}

pub(crate) fn is_documented_by_default(
    db: &ProjectDatabase,
    file: File,
    default_package_root: Option<&str>,
) -> bool {
    let path = relative_file_path(db, file);
    if path
        .split('/')
        .find(|component| !component.is_empty())
        .is_some_and(|component| matches!(component, "test" | "tests"))
    {
        return false;
    }

    default_package_root.is_none_or(|root| path_has_package_root(db, file, root))
}

fn path_has_package_root(db: &ProjectDatabase, file: File, package_root: &str) -> bool {
    let path = relative_file_path(db, file);
    let mut components = path.split('/').filter(|component| !component.is_empty());
    match components.next() {
        Some(first) if first == package_root => true,
        Some("src") => components.next() == Some(package_root),
        _ => false,
    }
}

fn is_private_module(module: &str) -> bool {
    module
        .split('.')
        .any(|component| !is_public_name(component))
}

fn is_public_name(name: &str) -> bool {
    !name.starts_with('_') || (name.starts_with("__") && name.ends_with("__"))
}
