//! This module walks the AST and collects a set of "semantic tokens" for a file
//! or a range within a file. Each semantic token provides a "token type" and zero
//! or more "modifiers". This information can be used by an editor to provide
//! color coding based on semantic meaning.
//!
//! Visual Studio has a very useful debugger that allows you to inspect the
//! semantic tokens for any given position in the code. Not only is this useful
//! to debug our semantic highlighting, it also allows easy comparison with
//! how Pylance (or other LSPs) highlight a certain token. You can open the scope inspector,
//! with the Command Palette (Command/Ctrl+Shift+P), then select the
//!  `Developer: Inspect Editor Tokens and Scopes` command.
//!
//! Current limitations and areas for future improvement:
//!
//! TODO: Need to handle semantic tokens within quoted annotations.
//!
//! TODO: Properties (or perhaps more generally, descriptor objects?) should be
//! classified as property tokens rather than just variables.
//!
//! TODO: Special forms like `Protocol` and `TypedDict` should probably be classified
//! as class tokens, but they are currently classified as variables.
//!
//! TODO: Type aliases (including those defined with the Python 3.12 "type" statement)
//! do not currently have a dedicated semantic token type, but they maybe should.
//!
//! TODO: Additional token modifiers might be added (e.g. for static methods,
//! abstract methods and classes).

use crate::Db;
use bitflags::bitflags;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{
    SourceOrderVisitor, TraversalSignal, walk_arguments, walk_expr,
    walk_interpolated_string_element, walk_stmt,
};
use ruff_python_ast::{
    self as ast, AnyNodeRef, ArgOrKeyword, BytesLiteral, Expr, InterpolatedStringElement, Stmt,
    StringLiteral, TypeParam,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use std::ops::Deref;
use ty_python_core::definition::{Definition, DefinitionKind, ParameterDefinitionNodeKind};
use ty_python_semantic::{
    HasType, SemanticModel,
    types::ide_support::{
        CallArgumentForm, call_argument_forms, definition_for_name,
        static_member_type_for_attribute,
    },
    types::{KnownInstanceType, MethodDecorator, SpecialFormType, Type, TypeVarKind},
};

/// Semantic token types supported by the language server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticTokenType {
    // This enum must be kept in sync with the SemanticTokenType below.
    Namespace,
    Class,
    Parameter,
    SelfParameter,
    ClsParameter,
    Variable,
    Property,
    Function,
    Method,
    Keyword,
    String,
    Number,
    Decorator,
    BuiltinConstant,
    TypeParameter,
}

impl SemanticTokenType {
    /// Returns all supported semantic token types as enum variants.
    pub const fn all() -> [SemanticTokenType; 15] {
        [
            SemanticTokenType::Namespace,
            SemanticTokenType::Class,
            SemanticTokenType::Parameter,
            SemanticTokenType::SelfParameter,
            SemanticTokenType::ClsParameter,
            SemanticTokenType::Variable,
            SemanticTokenType::Property,
            SemanticTokenType::Function,
            SemanticTokenType::Method,
            SemanticTokenType::Keyword,
            SemanticTokenType::String,
            SemanticTokenType::Number,
            SemanticTokenType::Decorator,
            SemanticTokenType::BuiltinConstant,
            SemanticTokenType::TypeParameter,
        ]
    }

    /// Converts this semantic token type to its LSP string representation.
    /// Some of these are standardized terms in the LSP specification,
    /// while others are specific to the ty language server. It's important
    /// to use the standardized ones where possible because clients can
    /// use these for standardized color coding and syntax highlighting.
    /// For details, refer to this LSP specification:
    /// <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#semanticTokenTypes>
    pub const fn as_lsp_concept(&self) -> &'static str {
        match self {
            SemanticTokenType::Namespace => "namespace",
            SemanticTokenType::Class => "class",
            SemanticTokenType::Parameter => "parameter",
            SemanticTokenType::SelfParameter => "selfParameter",
            SemanticTokenType::ClsParameter => "clsParameter",
            SemanticTokenType::Variable => "variable",
            SemanticTokenType::Property => "property",
            SemanticTokenType::Function => "function",
            SemanticTokenType::Method => "method",
            SemanticTokenType::Keyword => "keyword",
            SemanticTokenType::String => "string",
            SemanticTokenType::Number => "number",
            SemanticTokenType::Decorator => "decorator",
            SemanticTokenType::BuiltinConstant => "builtinConstant",
            SemanticTokenType::TypeParameter => "typeParameter",
        }
    }
}

bitflags! {
    /// Semantic token modifiers using bit flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SemanticTokenModifier: u32 {
        const DEFINITION = 1 << 0;
        const READONLY = 1 << 1;
        const ASYNC = 1 << 2;
        const DOCUMENTATION = 1 << 3;
    }
}

impl SemanticTokenModifier {
    /// Returns all supported token modifiers for LSP capabilities.
    /// Some of these are standardized terms in the LSP specification,
    /// while others may be specific to the ty language server. It's
    /// important to use the standardized ones where possible because
    /// clients can use these for standardized color coding and syntax
    /// highlighting. For details, refer to this LSP specification:
    /// <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#semanticTokenModifiers>
    pub fn all_names() -> Vec<&'static str> {
        vec!["definition", "readonly", "async", "documentation"]
    }
}

/// A semantic token with its position and classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub range: TextRange,
    pub token_type: SemanticTokenType,
    pub modifiers: SemanticTokenModifier,
}

impl Ranged for SemanticToken {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// The result of semantic tokenization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTokens {
    tokens: Vec<SemanticToken>,
}

impl SemanticTokens {
    /// Create a new `SemanticTokens` instance.
    pub fn new(tokens: Vec<SemanticToken>) -> Self {
        Self { tokens }
    }
}

impl Deref for SemanticTokens {
    type Target = [SemanticToken];

    fn deref(&self) -> &Self::Target {
        &self.tokens
    }
}

/// Generates semantic tokens for a Python file within the specified range.
/// Pass None to get tokens for the entire file.
pub fn semantic_tokens(db: &dyn Db, file: File, range: Option<TextRange>) -> SemanticTokens {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);

    let mut visitor = SemanticTokenVisitor::new(&model, range);
    visitor.expecting_docstring = true;
    visitor.visit_body(parsed.suite());

    SemanticTokens::new(visitor.tokens)
}

/// AST visitor that collects semantic tokens.
#[expect(clippy::struct_excessive_bools)]
struct SemanticTokenVisitor<'db> {
    model: &'db SemanticModel<'db>,
    tokens: Vec<SemanticToken>,
    in_class_scope: bool,
    in_type_form: bool,
    in_target_creating_definition: bool,
    in_docstring: bool,
    expecting_docstring: bool,
    range_filter: Option<TextRange>,
}

impl<'db> SemanticTokenVisitor<'db> {
    fn new(model: &'db SemanticModel<'db>, range_filter: Option<TextRange>) -> Self {
        Self {
            model,
            tokens: Vec::new(),
            in_class_scope: false,
            in_target_creating_definition: false,
            in_type_form: false,
            in_docstring: false,
            range_filter,
            expecting_docstring: false,
        }
    }

    fn add_token(
        &mut self,
        ranged: impl Ranged,
        token_type: SemanticTokenType,
        modifiers: SemanticTokenModifier,
    ) {
        let range = ranged.range();

        if range.is_empty() {
            return;
        }

        // Only emit tokens that intersect with the range filter, if one is specified
        if let Some(range_filter) = self.range_filter {
            // Only include ranges that have a non-empty overlap. Adjacent ranges
            // should be excluded.
            if range
                .intersect(range_filter)
                .is_none_or(TextRange::is_empty)
            {
                return;
            }
        }

        // Debug assertion to ensure tokens are added in file order
        debug_assert!(
            self.tokens
                .last()
                .is_none_or(|last| last.start() <= range.start()),
            "Tokens must be added in file order: previous token ends at {:?}, new token starts at {:?}",
            self.tokens.last().map(SemanticToken::start),
            range.start()
        );

        self.tokens.push(SemanticToken {
            range,
            token_type,
            modifiers,
        });
    }

    fn is_constant_name(name: &str) -> bool {
        name.chars()
            .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
            && name.len() > 1
    }

    fn classify_name(
        &self,
        name: &ast::ExprName,
    ) -> Option<(SemanticTokenType, SemanticTokenModifier)> {
        // First try to classify the token based on its definition kind.
        let definition = definition_for_name(
            self.model,
            name,
            ty_python_semantic::ImportAliasResolution::ResolveAliases,
        );

        if let Some(definition) = definition {
            let name_str = name.id.as_str();
            if let Some(classification) = self.classify_from_definition(definition, name_str) {
                return Some(classification);
            }
        }

        // Fall back to type-based classification.
        let ty = name.inferred_type(self.model).unwrap_or(Type::unknown());
        let name_str = name.id.as_str();
        self.classify_from_type_and_name_str(ty, name_str)
    }

    fn classify_from_definition(
        &self,
        definition: Definition,
        name_str: &str,
    ) -> Option<(SemanticTokenType, SemanticTokenModifier)> {
        let mut modifiers = SemanticTokenModifier::empty();
        let db = self.model.db();
        let model = SemanticModel::new(db, definition.file(db));

        match definition.kind(db) {
            DefinitionKind::Function(_) => {
                // Check if this is a method based on current scope
                if self.in_class_scope {
                    Some((SemanticTokenType::Method, modifiers))
                } else {
                    Some((SemanticTokenType::Function, modifiers))
                }
            }
            DefinitionKind::Class(_) => Some((SemanticTokenType::Class, modifiers)),
            DefinitionKind::TypeVar(_) | DefinitionKind::ParamSpec(_) => {
                Some((SemanticTokenType::TypeParameter, modifiers))
            }
            DefinitionKind::Parameter(ParameterDefinitionNodeKind::Parameter(parameter)) => {
                let parsed = parsed_module(db, definition.file(db));
                let ty = parameter.node(&parsed.load(db)).inferred_type(&model);

                if let Some(ty) = ty {
                    let type_var = match ty {
                        Type::TypeVar(type_var) => Some((type_var, false)),
                        Type::SubclassOf(subclass_of) => {
                            subclass_of.into_type_var().map(|var| (var, true))
                        }
                        _ => None,
                    };

                    if let Some((type_var, is_cls)) = type_var
                        && matches!(type_var.typevar(db).kind(db), TypeVarKind::TypingSelf)
                    {
                        let kind = if is_cls {
                            SemanticTokenType::ClsParameter
                        } else {
                            SemanticTokenType::SelfParameter
                        };

                        return Some((kind, modifiers));
                    }
                }

                Some((SemanticTokenType::Parameter, modifiers))
            }
            DefinitionKind::Parameter(_) => Some((SemanticTokenType::Parameter, modifiers)),
            DefinitionKind::TypeAlias(_) => Some((SemanticTokenType::TypeParameter, modifiers)),
            DefinitionKind::Import(_)
            | DefinitionKind::ImportFrom(_)
            | DefinitionKind::StarImport(_) => {
                // For imports, return None to fall back to type-based classification
                // This allows imported names to be classified based on what they actually are
                // (e.g., imported classes as Class, imported functions as Function, etc.)
                None
            }
            _ => {
                // For other definition kinds (assignments, etc.), apply constant naming convention
                if Self::is_constant_name(name_str) {
                    modifiers |= SemanticTokenModifier::READONLY;
                }

                let parsed = parsed_module(db, definition.file(db));
                let parsed = parsed.load(db);
                let value = match definition.kind(db) {
                    DefinitionKind::Assignment(assignment) => Some(assignment.value(&parsed)),
                    _ => None,
                };

                if let Some(value) = value
                    && let Some(value_ty) = value.inferred_type(&model)
                {
                    if matches!(value_ty, Type::KnownInstance(KnownInstanceType::TypeVar(_))) {
                        modifiers.remove(SemanticTokenModifier::READONLY);
                        return Some((SemanticTokenType::TypeParameter, modifiers));
                    }

                    if value_ty.is_class_literal()
                        || value_ty.is_subclass_of()
                        || value_ty.is_generic_alias()
                    {
                        return Some((SemanticTokenType::Class, modifiers));
                    }
                }

                Some((SemanticTokenType::Variable, modifiers))
            }
        }
    }

    fn classify_from_type_and_name_str(
        &self,
        ty: Type,
        name_str: &str,
    ) -> Option<(SemanticTokenType, SemanticTokenModifier)> {
        if ty.is_unknown() {
            return None;
        }

        let mut modifiers = SemanticTokenModifier::empty();

        if let Some(classification) = self.classify_type_form_expr(ty) {
            return Some(classification);
        }

        Some(match ty {
            Type::ClassLiteral(_) => (SemanticTokenType::Class, modifiers),
            Type::TypeVar(_) => (SemanticTokenType::TypeParameter, modifiers),
            Type::KnownInstance(KnownInstanceType::TypeVar(_)) => {
                (SemanticTokenType::TypeParameter, modifiers)
            }
            Type::FunctionLiteral(_) => {
                // Check if this is a method based on current scope
                if self.in_class_scope {
                    (SemanticTokenType::Method, modifiers)
                } else {
                    (SemanticTokenType::Function, modifiers)
                }
            }
            Type::BoundMethod(_) => (SemanticTokenType::Method, modifiers),
            Type::ModuleLiteral(_) => (SemanticTokenType::Namespace, modifiers),
            _ => {
                // Check for constant naming convention
                if Self::is_constant_name(name_str) {
                    modifiers |= SemanticTokenModifier::READONLY;
                }
                // For other types (variables, modules, etc.), assume variable
                (SemanticTokenType::Variable, modifiers)
            }
        })
    }

    fn classify_type_form_expr(
        &self,
        ty: Type,
    ) -> Option<(SemanticTokenType, SemanticTokenModifier)> {
        if !self.in_type_form {
            return None;
        }

        // In type-form contexts, these types all denote class-like type expressions that should be
        // highlighted like `int` in `x: int`, even if their inferred type is instance-shaped.
        match ty {
            Type::ClassLiteral(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_) => {
                Some((SemanticTokenType::Class, SemanticTokenModifier::empty()))
            }
            _ => None,
        }
    }

    fn classify_from_type_for_attribute(
        &self,
        ty: Type,
        attr_name: &ast::Identifier,
    ) -> Option<(SemanticTokenType, SemanticTokenModifier)> {
        enum UnifiedTokenType {
            None,
            /// All types have the same semantic token type
            Uniform(SemanticTokenType),
            /// The elements have different semantic token types
            Fallback,
        }

        impl UnifiedTokenType {
            fn add(&mut self, ty: SemanticTokenType) {
                *self = match self {
                    Self::None => Self::Uniform(ty),
                    Self::Uniform(current) if *current == ty => Self::Uniform(ty),
                    Self::Uniform(_) | Self::Fallback => Self::Fallback,
                }
            }

            fn into_semantic_token_type(self) -> Option<SemanticTokenType> {
                match self {
                    UnifiedTokenType::None | UnifiedTokenType::Fallback => None,
                    UnifiedTokenType::Uniform(ty) => Some(ty),
                }
            }
        }

        if ty.is_unknown() {
            return None;
        }

        let db = self.model.db();
        let attr_name_str = attr_name.id.as_str();
        let mut modifiers = SemanticTokenModifier::empty();

        if let Some(classification) = self.classify_type_form_expr(ty) {
            return Some(classification);
        }

        let elements = if let Some(union) = ty.as_union() {
            union.elements(db)
        } else {
            std::slice::from_ref(&ty)
        };

        let mut token_type = UnifiedTokenType::None;
        let mut all_properties_are_readonly = true;

        for element in elements {
            // Classify based on the inferred type of the attribute
            match element {
                Type::ClassLiteral(_) => {
                    token_type.add(SemanticTokenType::Class);
                }
                Type::FunctionLiteral(_) => {
                    // This is a function accessed as an attribute, likely a method
                    token_type.add(SemanticTokenType::Method);
                }
                Type::BoundMethod(_) | Type::KnownBoundMethod(_) => {
                    // Method bound to an instance
                    token_type.add(SemanticTokenType::Method);
                }
                Type::ModuleLiteral(_) => {
                    // Module accessed as an attribute (e.g., from os import path)
                    token_type.add(SemanticTokenType::Namespace);
                }
                Type::PropertyInstance(property) => {
                    token_type.add(SemanticTokenType::Property);
                    all_properties_are_readonly &= property.setter(db).is_none();
                }
                _ => {
                    token_type = UnifiedTokenType::Fallback;
                }
            }
        }

        if let Some(uniform) = token_type.into_semantic_token_type() {
            if uniform == SemanticTokenType::Property && all_properties_are_readonly {
                modifiers |= SemanticTokenModifier::READONLY;
            }
            return Some((uniform, modifiers));
        }

        // Check for constant naming convention
        if Self::is_constant_name(attr_name_str) {
            modifiers |= SemanticTokenModifier::READONLY;
        }

        // For other types (variables, constants, etc.), classify as variable
        // Should this always be property?
        Some((SemanticTokenType::Variable, modifiers))
    }

    fn classify_parameter(
        &self,
        _param: &ast::Parameter,
        is_first: bool,
        func: &ast::StmtFunctionDef,
    ) -> SemanticTokenType {
        if is_first && self.in_class_scope {
            let method_decorator = func
                .inferred_type(self.model)
                .and_then(Type::as_function_literal)
                .and_then(|function_ty| {
                    MethodDecorator::try_from_fn_type(self.model.db(), function_ty)
                })
                .unwrap_or_default();

            match method_decorator {
                MethodDecorator::StaticMethod => SemanticTokenType::Parameter,
                MethodDecorator::ClassMethod => SemanticTokenType::ClsParameter,
                MethodDecorator::None => SemanticTokenType::SelfParameter,
            }
        } else {
            SemanticTokenType::Parameter
        }
    }

    fn add_dotted_name_tokens(&mut self, name: &ast::Identifier, token_type: SemanticTokenType) {
        let name_str = name.id.as_str();
        let name_start = name.start();

        // Split the dotted name and calculate positions for each part
        let mut current_offset = TextSize::default();
        for part in name_str.split('.') {
            if !part.is_empty() {
                self.add_token(
                    TextRange::at(name_start + current_offset, part.text_len()),
                    token_type,
                    SemanticTokenModifier::empty(),
                );
            }
            // Move past this part and the dot
            current_offset += part.text_len() + '.'.text_len();
        }
    }

    fn classify_from_alias_type(
        &self,
        ty: Type,
        local_name: &ast::Identifier,
    ) -> Option<(SemanticTokenType, SemanticTokenModifier)> {
        self.classify_from_type_and_name_str(ty, local_name.id.as_str())
    }

    // Visit parameters for a function or lambda expression and classify
    // them as parameters, selfParameter, or clsParameter as appropriate.
    fn visit_parameters(
        &mut self,
        parameters: &ast::Parameters,
        func: Option<&ast::StmtFunctionDef>,
    ) {
        for (param_index, any_param) in parameters.iter_source_order().enumerate() {
            let parameter = any_param.as_parameter();

            let token_type = match any_param {
                // For non-variadic parameters in function defs, preserve self/cls classification.
                ast::AnyParameterRef::NonVariadic(_) => func
                    .map_or(SemanticTokenType::Parameter, |func| {
                        self.classify_parameter(parameter, param_index == 0, func)
                    }),
                ast::AnyParameterRef::Variadic(_) => SemanticTokenType::Parameter,
            };

            self.add_token(
                parameter.name.range(),
                token_type,
                SemanticTokenModifier::DEFINITION,
            );

            // Handle parameter type annotations
            if let Some(annotation) = &parameter.annotation {
                self.visit_annotation(annotation);
            }

            if let Some(default) = any_param.default() {
                self.visit_expr(default);
            }
        }
    }

    fn visit_expr_with_type_form(&mut self, expr: &Expr, in_type_form: bool) {
        let prev_in_type_form = self.in_type_form;
        self.in_type_form = in_type_form;
        self.visit_expr(expr);
        self.in_type_form = prev_in_type_form;
    }

    fn visit_value_expression(&mut self, expr: &Expr) {
        self.visit_expr_with_type_form(expr, false);
    }

    fn visit_annotated_arguments(&mut self, slice: &Expr) {
        let ast::Expr::Tuple(tuple) = slice else {
            self.visit_annotation(slice);
            return;
        };

        let Some((annotation, metadata)) = tuple.elts.split_first() else {
            self.visit_annotation(slice);
            return;
        };

        self.visit_annotation(annotation);

        for metadata_element in metadata {
            self.visit_value_expression(metadata_element);
        }
    }
}

impl SourceOrderVisitor<'_> for SemanticTokenVisitor<'_> {
    fn enter_node(&mut self, node: AnyNodeRef<'_>) -> TraversalSignal {
        // If we have a range filter and this node doesn't intersect, skip it
        // and all its children as an optimization
        if let Some(range_filter) = self.range_filter {
            if node.range().intersect(range_filter).is_none() {
                return TraversalSignal::Skip;
            }
        }
        TraversalSignal::Traverse
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        let expecting_docstring = self.expecting_docstring;
        self.expecting_docstring = false;
        match stmt {
            ast::Stmt::FunctionDef(func) => {
                // Visit decorator expressions
                for decorator in &func.decorator_list {
                    self.visit_decorator(decorator);
                }

                // Function name
                self.add_token(
                    func.name.range(),
                    if self.in_class_scope {
                        SemanticTokenType::Method
                    } else {
                        SemanticTokenType::Function
                    },
                    if func.is_async {
                        SemanticTokenModifier::DEFINITION | SemanticTokenModifier::ASYNC
                    } else {
                        SemanticTokenModifier::DEFINITION
                    },
                );

                // Type parameters (Python 3.12+ syntax)
                if let Some(type_params) = &func.type_params {
                    for type_param in &type_params.type_params {
                        self.visit_type_param(type_param);
                    }
                }

                self.visit_parameters(&func.parameters, Some(func));

                // Handle return type annotation
                if let Some(returns) = &func.returns {
                    self.visit_annotation(returns);
                }

                // Clear the in_class_scope flag so inner functions
                // are not treated as methods
                let prev_in_class = self.in_class_scope;

                self.in_class_scope = false;
                self.expecting_docstring = true;
                self.visit_body(&func.body);
                self.expecting_docstring = false;
                self.in_class_scope = prev_in_class;
            }
            ast::Stmt::ClassDef(class) => {
                // Visit decorator expressions
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }

                // Class name
                self.add_token(
                    class.name.range(),
                    SemanticTokenType::Class,
                    SemanticTokenModifier::DEFINITION,
                );

                // Type parameters (Python 3.12+ syntax)
                if let Some(type_params) = &class.type_params {
                    for type_param in &type_params.type_params {
                        self.visit_type_param(type_param);
                    }
                }

                // Handle base classes and type annotations in inheritance
                if let Some(arguments) = &class.arguments {
                    walk_arguments(self, arguments);
                }

                let prev_in_class = self.in_class_scope;
                self.in_class_scope = true;
                self.expecting_docstring = true;
                self.visit_body(&class.body);
                self.expecting_docstring = false;
                self.in_class_scope = prev_in_class;
            }
            ast::Stmt::TypeAlias(type_alias) => {
                // Type alias name
                self.add_token(
                    type_alias.name.range(),
                    SemanticTokenType::Class,
                    SemanticTokenModifier::DEFINITION,
                );

                // Type parameters (Python 3.12+ syntax)
                if let Some(type_params) = &type_alias.type_params {
                    for type_param in &type_params.type_params {
                        self.visit_type_param(type_param);
                    }
                }

                self.visit_annotation(&type_alias.value);
            }
            ast::Stmt::Import(import) => {
                for alias in &import.names {
                    // Create separate tokens for each part of a dotted module name
                    self.add_dotted_name_tokens(&alias.name, SemanticTokenType::Namespace);

                    if let Some(asname) = &alias.asname {
                        self.add_token(
                            asname.range(),
                            SemanticTokenType::Namespace,
                            SemanticTokenModifier::empty(),
                        );
                    }
                }
            }
            ast::Stmt::ImportFrom(import) => {
                if let Some(module) = &import.module {
                    // Create separate tokens for each part of a dotted module name
                    self.add_dotted_name_tokens(module, SemanticTokenType::Namespace);
                }
                for alias in &import.names {
                    // Get the type of the imported name
                    let ty = alias.inferred_type(self.model).unwrap_or(Type::unknown());
                    if let Some((token_type, modifiers)) =
                        self.classify_from_alias_type(ty, &alias.name)
                    {
                        // Add token for the imported name (Y in "from X import Y" or "from X import Y as Z")
                        self.add_token(&alias.name, token_type, modifiers);

                        // For aliased imports (from X import Y as Z), also add a token for the alias Z
                        if let Some(asname) = &alias.asname {
                            self.add_token(asname, token_type, modifiers);
                        }
                    }
                }
            }
            ast::Stmt::Nonlocal(nonlocal_stmt) => {
                // Handle nonlocal statements - classify identifiers as variables
                for identifier in &nonlocal_stmt.names {
                    self.add_token(
                        identifier.range(),
                        SemanticTokenType::Variable,
                        SemanticTokenModifier::empty(),
                    );
                }
            }
            ast::Stmt::Global(global_stmt) => {
                // Handle global statements - classify identifiers as variables
                for identifier in &global_stmt.names {
                    self.add_token(
                        identifier.range(),
                        SemanticTokenType::Variable,
                        SemanticTokenModifier::empty(),
                    );
                }
            }
            ast::Stmt::Assign(assignment) => {
                self.in_target_creating_definition = true;
                for element in &assignment.targets {
                    self.visit_expr(element);
                }
                self.in_target_creating_definition = false;

                self.visit_expr(&assignment.value);
                self.expecting_docstring = true;
            }
            ast::Stmt::AnnAssign(assignment) => {
                self.in_target_creating_definition = true;
                self.visit_expr(&assignment.target);
                self.in_target_creating_definition = false;

                self.visit_annotation(&assignment.annotation);

                if let Some(value) = &assignment.value {
                    // PEP 613 alias values are type forms even though they appear as annotated
                    // assignments rather than dedicated `type` statements.
                    if matches!(
                        assignment.annotation.inferred_type(self.model),
                        Some(Type::SpecialForm(SpecialFormType::TypeAlias))
                    ) {
                        self.visit_annotation(value);
                    } else {
                        self.visit_expr(value);
                    }
                }
                self.expecting_docstring = true;
            }
            ast::Stmt::For(for_stmt) => {
                self.in_target_creating_definition = true;
                self.visit_expr(&for_stmt.target);
                self.in_target_creating_definition = false;

                self.visit_expr(&for_stmt.iter);
                self.visit_body(&for_stmt.body);
                self.visit_body(&for_stmt.orelse);
            }
            ast::Stmt::With(with_stmt) => {
                for item in &with_stmt.items {
                    self.visit_expr(&item.context_expr);
                    if let Some(expr) = &item.optional_vars {
                        self.in_target_creating_definition = true;
                        self.visit_expr(expr);
                        self.in_target_creating_definition = false;
                    }
                }

                self.visit_body(&with_stmt.body);
            }
            ast::Stmt::Try(try_stmt) => {
                self.visit_body(&try_stmt.body);
                for handler in &try_stmt.handlers {
                    match handler {
                        ast::ExceptHandler::ExceptHandler(except_handler) => {
                            if let Some(expr) = &except_handler.type_ {
                                self.visit_expr(expr);
                            }
                            if let Some(name) = &except_handler.name {
                                self.add_token(
                                    name.range(),
                                    SemanticTokenType::Variable,
                                    SemanticTokenModifier::DEFINITION,
                                );
                            }
                            self.visit_body(&except_handler.body);
                        }
                    }
                }
                self.visit_body(&try_stmt.orelse);
                self.visit_body(&try_stmt.finalbody);
            }
            ast::Stmt::Expr(expr) => {
                if expecting_docstring && expr.value.is_string_literal_expr() {
                    self.in_docstring = true;
                }
                walk_stmt(self, stmt);
                self.in_docstring = false;
            }
            _ => {
                // For all other statement types, let the default visitor handle them
                walk_stmt(self, stmt);
            }
        }
    }

    /// Visit an annotation or other expression that should be interpreted as a type form.
    fn visit_annotation(&mut self, expr: &'_ Expr) {
        self.visit_expr_with_type_form(expr, true);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            ast::Expr::Name(name) => {
                if let Some((token_type, mut modifiers)) = self.classify_name(name) {
                    if self.in_target_creating_definition && name.ctx.is_store() {
                        modifiers |= SemanticTokenModifier::DEFINITION;
                    }
                    self.add_token(name, token_type, modifiers);
                }
                walk_expr(self, expr);
            }
            ast::Expr::Attribute(attr) => {
                // Visit the base expression first (e.g., 'os' in 'os.path')
                self.visit_expr(&attr.value);

                // Then add token for the attribute name (e.g., 'path' in 'os.path')
                let ty = static_member_type_for_attribute(self.model, attr)
                    .unwrap_or_else(|| expr.inferred_type(self.model).unwrap_or(Type::unknown()));
                if let Some((token_type, modifiers)) =
                    self.classify_from_type_for_attribute(ty, &attr.attr)
                {
                    self.add_token(&attr.attr, token_type, modifiers);
                }
            }
            ast::Expr::NumberLiteral(_) => {
                self.add_token(
                    expr.range(),
                    SemanticTokenType::Number,
                    SemanticTokenModifier::empty(),
                );
            }
            ast::Expr::BooleanLiteral(_) => {
                self.add_token(
                    expr.range(),
                    SemanticTokenType::BuiltinConstant,
                    SemanticTokenModifier::empty(),
                );
            }
            ast::Expr::NoneLiteral(_) => {
                self.add_token(
                    expr.range(),
                    SemanticTokenType::BuiltinConstant,
                    SemanticTokenModifier::empty(),
                );
            }
            ast::Expr::Lambda(lambda) => {
                // Handle lambda parameters
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters, None);
                }

                // Visit the lambda body
                self.visit_expr(&lambda.body);
            }

            ast::Expr::Named(named) => {
                let prev_in_target = self.in_target_creating_definition;
                self.in_target_creating_definition = true;
                self.visit_expr(&named.target);
                self.in_target_creating_definition = prev_in_target;

                self.visit_expr(&named.value);
            }
            ast::Expr::StringLiteral(string_expr) => {
                // Highlight the sub-AST of a string annotation
                if let Some((sub_ast, sub_model)) = self.model.enter_string_annotation(string_expr)
                {
                    let mut sub_visitor = SemanticTokenVisitor::new(&sub_model, self.range_filter);
                    sub_visitor.visit_expr(sub_ast.expr());
                    self.tokens.extend(sub_visitor.tokens);
                } else {
                    walk_expr(self, expr);
                }
            }
            ast::Expr::Subscript(subscript)
                if matches!(
                    subscript.value.inferred_type(self.model),
                    Some(Type::SpecialForm(SpecialFormType::Annotated))
                ) =>
            {
                self.visit_expr(subscript.value.as_ref());
                self.visit_annotated_arguments(subscript.slice.as_ref());
            }
            ast::Expr::Call(call) => {
                self.visit_expr(call.func.as_ref());

                // Determine whether each argument should be considered a type form or a value
                // based on the position.
                let argument_forms = call_argument_forms(self.model, call);
                for (argument, form) in call.arguments.iter_source_order().zip(argument_forms) {
                    match form {
                        CallArgumentForm::Type => self.visit_annotation(argument.value()),
                        CallArgumentForm::Unknown | CallArgumentForm::Value => match argument {
                            ArgOrKeyword::Arg(argument) => self.visit_expr(argument),
                            ArgOrKeyword::Keyword(keyword) => self.visit_keyword(keyword),
                        },
                    }
                }
            }
            _ => {
                // For all other expression types, let the default visitor handle them
                walk_expr(self, expr);
            }
        }
    }

    fn visit_string_literal(&mut self, string_literal: &StringLiteral) {
        // Emit a semantic token for this string literal part
        let modifiers = if self.in_docstring {
            SemanticTokenModifier::DOCUMENTATION
        } else {
            SemanticTokenModifier::empty()
        };
        self.add_token(string_literal.range(), SemanticTokenType::String, modifiers);
    }

    fn visit_bytes_literal(&mut self, bytes_literal: &BytesLiteral) {
        // Emit a semantic token for this bytes literal part
        self.add_token(
            bytes_literal.range(),
            SemanticTokenType::String,
            SemanticTokenModifier::empty(),
        );
    }

    fn visit_interpolated_string_element(
        &mut self,
        interpolated_string_element: &InterpolatedStringElement,
    ) {
        match interpolated_string_element {
            InterpolatedStringElement::Literal(literal) => {
                // Emit a String token for literal parts of f-strings/t-strings
                self.add_token(
                    literal.range(),
                    SemanticTokenType::String,
                    SemanticTokenModifier::empty(),
                );
            }
            InterpolatedStringElement::Interpolation(_) => {
                // The default walker handles visiting the expression and format spec
                walk_interpolated_string_element(self, interpolated_string_element);
            }
        }
    }

    /// Visit decorators, handling simple name decorators vs complex expressions
    fn visit_decorator(&mut self, decorator: &ast::Decorator) {
        match &decorator.expression {
            ast::Expr::Name(name) => {
                // Simple decorator like @staticmethod - use Decorator token type
                self.add_token(
                    name.range(),
                    SemanticTokenType::Decorator,
                    SemanticTokenModifier::empty(),
                );
            }
            _ => {
                // Complex decorator like @app.route("/path") - use normal expression rules
                self.visit_expr(&decorator.expression);
            }
        }
    }

    fn visit_type_param(&mut self, type_param: &TypeParam) {
        // Emit token for the type parameter name
        let name_range = type_param.name().range();
        self.add_token(
            name_range,
            SemanticTokenType::TypeParameter,
            SemanticTokenModifier::DEFINITION,
        );

        // Visit bound expression (for TypeVar)
        // TODO: We don't call `walk_type_param` here, because, as of today (20th Oct 2025),
        // `walk_type_param` calls `visit_expr` instead of `visit_annotation`.
        match type_param {
            TypeParam::TypeVar(type_var) => {
                if let Some(bound) = &type_var.bound {
                    self.visit_annotation(bound);
                }
                if let Some(default) = &type_var.default {
                    self.visit_annotation(default);
                }
            }
            TypeParam::ParamSpec(param_spec) => {
                if let Some(default) = &param_spec.default {
                    self.visit_annotation(default);
                }
            }
            TypeParam::TypeVarTuple(type_var_tuple) => {
                if let Some(default) = &type_var_tuple.default {
                    self.visit_annotation(default);
                }
            }
        }
    }

    fn visit_except_handler(&mut self, except_handler: &ast::ExceptHandler) {
        match except_handler {
            ast::ExceptHandler::ExceptHandler(handler) => {
                // Visit the exception type expression if present
                if let Some(type_expr) = &handler.type_ {
                    self.visit_expr(type_expr);
                }

                // Handle the exception variable name (after "as")
                if let Some(name) = &handler.name {
                    self.add_token(
                        name.range(),
                        SemanticTokenType::Variable,
                        SemanticTokenModifier::empty(),
                    );
                }

                // Visit the handler body
                self.visit_body(&handler.body);
            }
        }
    }

    fn visit_pattern(&mut self, pattern: &ast::Pattern) {
        match pattern {
            ast::Pattern::MatchAs(pattern_as) => {
                // Visit the nested pattern first to maintain source order
                if let Some(nested_pattern) = &pattern_as.pattern {
                    self.visit_pattern(nested_pattern);
                }

                // Now add the "as" variable name token
                if let Some(name) = &pattern_as.name {
                    self.add_token(
                        name.range(),
                        SemanticTokenType::Variable,
                        SemanticTokenModifier::empty(),
                    );
                }
            }
            ast::Pattern::MatchMapping(pattern_mapping) => {
                // `**rest` can appear before or after the key-value pairs
                // (the parser can produce either AST, but emits an
                // invalid-syntax error for the former).
                let rest_before_keys = pattern_mapping.rest.as_ref().filter(|rest| {
                    pattern_mapping
                        .keys
                        .first()
                        .is_some_and(|key| rest.start() < key.start())
                });
                let rest_after_keys = pattern_mapping.rest.as_ref().filter(|rest| {
                    pattern_mapping
                        .keys
                        .first()
                        .is_none_or(|key| rest.start() >= key.start())
                });

                if let Some(rest_name) = rest_before_keys {
                    self.add_token(
                        rest_name.range(),
                        SemanticTokenType::Variable,
                        SemanticTokenModifier::empty(),
                    );
                }

                for (key, nested_pattern) in
                    pattern_mapping.keys.iter().zip(&pattern_mapping.patterns)
                {
                    self.visit_expr(key);
                    self.visit_pattern(nested_pattern);
                }

                if let Some(rest_name) = rest_after_keys {
                    self.add_token(
                        rest_name.range(),
                        SemanticTokenType::Variable,
                        SemanticTokenModifier::empty(),
                    );
                }
            }
            ast::Pattern::MatchStar(pattern_star) => {
                // Just the one ident here
                if let Some(rest_name) = &pattern_star.name {
                    self.add_token(
                        rest_name.range(),
                        SemanticTokenType::Variable,
                        SemanticTokenModifier::empty(),
                    );
                }
            }
            _ => {
                // For all other pattern types, use the default walker
                ruff_python_ast::visitor::source_order::walk_pattern(self, pattern);
            }
        }
    }

    fn visit_comprehension(&mut self, comp: &ast::Comprehension) {
        self.in_target_creating_definition = true;
        self.visit_expr(&comp.target);
        self.in_target_creating_definition = false;

        self.visit_expr(&comp.iter);
        for if_clause in &comp.ifs {
            self.visit_expr(if_clause);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use insta::assert_snapshot;
    use ruff_db::{
        files::system_path_to_file,
        system::{DbWithWritableSystem, SystemPath, SystemPathBuf},
    };
    use ty_project::ProjectMetadata;

    #[test]
    fn semantic_tokens_basic() {
        let test = SemanticTokenTest::new("def foo(): pass");

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#""foo" @ 4..7: Function [definition]"#);
    }

    #[test]
    fn semantic_tokens_class() {
        let test = SemanticTokenTest::new("class MyClass: pass");

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#""MyClass" @ 6..13: Class [definition]"#);
    }

    #[test]
    fn semantic_tokens_class_args() {
        // This used to cause a panic because of an incorrect
        // insertion-order when visiting arguments inside
        // class definitions.
        let test = SemanticTokenTest::new("class Foo(m=x, m)");

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#""Foo" @ 6..9: Class [definition]"#);
    }

    #[test]
    fn semantic_tokens_annotated_metadata() {
        let test = SemanticTokenTest::new(
            "
from typing import Annotated

class Metadata:
    field = 1

def f(x: Annotated[int, Metadata.field]): ...
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "Annotated" @ 20..29: Variable
        "Metadata" @ 37..45: Class [definition]
        "field" @ 51..56: Variable [definition]
        "1" @ 59..60: Number
        "f" @ 66..67: Function [definition]
        "x" @ 68..69: Parameter [definition]
        "Annotated" @ 71..80: Variable
        "int" @ 81..84: Class
        "Metadata" @ 86..94: Class
        "field" @ 95..100: Variable
        "#);
    }

    #[test]
    fn semantic_tokens_match_class_pattern_keyword_before_positional() {
        // Regression test for https://github.com/astral-sh/ty/issues/2417
        // This used to cause a panic because keyword patterns and positional
        // patterns in a match class were not visited in source order.
        let test = SemanticTokenTest::new(
            "
import ast
def f(x: ast.AST):
    match x:
        case ast.Attribute(value=ast.Name(id), attr):
            pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "ast" @ 8..11: Namespace
        "f" @ 16..17: Function [definition]
        "x" @ 18..19: Parameter [definition]
        "ast" @ 21..24: Namespace
        "AST" @ 25..28: Class
        "x" @ 41..42: Parameter
        "ast" @ 57..60: Namespace
        "Attribute" @ 61..70: Class
        "ast" @ 77..80: Namespace
        "Name" @ 81..85: Class
        "id" @ 86..88: Variable
        "attr" @ 91..95: Variable
        "#);
    }

    #[test]
    fn semantic_tokens_match_mapping_pattern_rest_before_keys() {
        let test = SemanticTokenTest::new(
            "
def f(x):
    match x:
        case {**rest, 'key': value}:
            pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "f" @ 5..6: Function [definition]
        "x" @ 7..8: Parameter [definition]
        "x" @ 21..22: Parameter
        "rest" @ 40..44: Variable
        "'key'" @ 46..51: String
        "value" @ 53..58: Variable
        "#);
    }

    #[test]
    fn semantic_tokens_variables() {
        let test = SemanticTokenTest::new(
            "
x = 42
y = 'hello'
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 1..2: Variable [definition]
        "42" @ 5..7: Number
        "y" @ 8..9: Variable [definition]
        "'hello'" @ 12..19: String
        "#);
    }

    #[test]
    fn semantic_tokens_legacy_typevar() {
        let test = SemanticTokenTest::new(
            r#"
from typing import Generic, TypeVar

KT = TypeVar("KT")

class Box(Generic[KT]):
    value: KT
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "Generic" @ 20..27: Variable
        "TypeVar" @ 29..36: Class
        "KT" @ 38..40: TypeParameter [definition]
        "TypeVar" @ 43..50: Class
        "\"KT\"" @ 51..55: String
        "Box" @ 64..67: Class [definition]
        "Generic" @ 68..75: Variable
        "KT" @ 76..78: TypeParameter
        "value" @ 86..91: Variable [definition]
        "KT" @ 93..95: TypeParameter
        "#);
    }

    #[test]
    fn semantic_tokens_legacy_paramspec() {
        let test = SemanticTokenTest::new(
            r#"
from typing import Callable, ParamSpec

P = ParamSpec("P")

def decorator(func: Callable[P, int]) -> Callable[P, str]: ...
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "Callable" @ 20..28: Variable
        "ParamSpec" @ 30..39: Class
        "P" @ 41..42: TypeParameter [definition]
        "ParamSpec" @ 45..54: Class
        "\"P\"" @ 55..58: String
        "decorator" @ 65..74: Function [definition]
        "func" @ 75..79: Parameter [definition]
        "Callable" @ 81..89: Variable
        "P" @ 90..91: TypeParameter
        "int" @ 93..96: Class
        "Callable" @ 102..110: Variable
        "P" @ 111..112: TypeParameter
        "str" @ 114..117: Class
        "#);
    }

    #[test]
    fn semantic_tokens_walrus() {
        let test = SemanticTokenTest::new(
            "
if x := 42:
    y = 'hello'
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 4..5: Variable [definition]
        "42" @ 9..11: Number
        "y" @ 17..18: Variable [definition]
        "'hello'" @ 21..28: String
        "#);
    }

    #[test]
    fn semantic_tokens_self_parameter() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    def method(self, x):
        self.x = 10

    def method_unidiomatic_self(self2):
        print(self2.x))
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "method" @ 24..30: Method [definition]
        "self" @ 31..35: SelfParameter [definition]
        "x" @ 37..38: Parameter [definition]
        "self" @ 49..53: SelfParameter
        "x" @ 54..55: Variable
        "10" @ 58..60: Number
        "method_unidiomatic_self" @ 70..93: Method [definition]
        "self2" @ 94..99: SelfParameter [definition]
        "print" @ 110..115: Function
        "self2" @ 116..121: SelfParameter
        "x" @ 122..123: Variable
        "#);
    }

    #[test]
    fn semantic_tokens_cls_parameter() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    @classmethod
    def method(cls, x): print(cls)
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "classmethod" @ 21..32: Decorator
        "method" @ 41..47: Method [definition]
        "cls" @ 48..51: ClsParameter [definition]
        "x" @ 53..54: Parameter [definition]
        "print" @ 57..62: Function
        "cls" @ 63..66: ClsParameter
        "#);
    }

    #[test]
    fn semantic_tokens_staticmethod_parameter() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    @staticmethod
    def method(x, y): pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "staticmethod" @ 21..33: Decorator
        "method" @ 42..48: Method [definition]
        "x" @ 49..50: Parameter [definition]
        "y" @ 52..53: Parameter [definition]
        "#);
    }

    #[test]
    fn semantic_tokens_aliased_staticmethod_parameter() {
        let test = SemanticTokenTest::new(
            "
sm = staticmethod

class MyClass:
    @sm
    def method(x, y): pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "sm" @ 1..3: Class [definition]
        "staticmethod" @ 6..18: Class
        "MyClass" @ 26..33: Class [definition]
        "sm" @ 40..42: Decorator
        "method" @ 51..57: Method [definition]
        "x" @ 58..59: Parameter [definition]
        "y" @ 61..62: Parameter [definition]
        "#);
    }

    #[test]
    fn semantic_tokens_aliased_classmethod_parameter() {
        let test = SemanticTokenTest::new(
            "
cm = classmethod

class MyClass:
    @cm
    def method(cls, x): pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "cm" @ 1..3: Class [definition]
        "classmethod" @ 6..17: Class
        "MyClass" @ 25..32: Class [definition]
        "cm" @ 39..41: Decorator
        "method" @ 50..56: Method [definition]
        "cls" @ 57..60: ClsParameter [definition]
        "x" @ 62..63: Parameter [definition]
        "#);
    }

    #[test]
    fn semantic_tokens_custom_self_cls_names() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    def method(instance, x): pass
    @classmethod
    def other(klass, y): print(klass)
    def complex_method(instance, posonly, /, regular, *args, kwonly, **kwargs): pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "method" @ 24..30: Method [definition]
        "instance" @ 31..39: SelfParameter [definition]
        "x" @ 41..42: Parameter [definition]
        "classmethod" @ 55..66: Decorator
        "other" @ 75..80: Method [definition]
        "klass" @ 81..86: ClsParameter [definition]
        "y" @ 88..89: Parameter [definition]
        "print" @ 92..97: Function
        "klass" @ 98..103: ClsParameter
        "complex_method" @ 113..127: Method [definition]
        "instance" @ 128..136: SelfParameter [definition]
        "posonly" @ 138..145: Parameter [definition]
        "regular" @ 150..157: Parameter [definition]
        "args" @ 160..164: Parameter [definition]
        "kwonly" @ 166..172: Parameter [definition]
        "kwargs" @ 176..182: Parameter [definition]
        "#);
    }

    #[test]
    fn semantic_tokens_modifiers() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    CONSTANT = 42
    async def method(self): pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "CONSTANT" @ 20..28: Variable [definition, readonly]
        "42" @ 31..33: Number
        "method" @ 48..54: Method [definition, async]
        "self" @ 55..59: SelfParameter [definition]
        "#);
    }

    #[test]
    fn semantic_classification_vs_heuristic() {
        let test = SemanticTokenTest::new(
            "
import sys
class MyClass:
    pass

def my_function():
    return 42

x = MyClass()
y = my_function()
z = sys.version
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "sys" @ 8..11: Namespace
        "MyClass" @ 18..25: Class [definition]
        "my_function" @ 41..52: Function [definition]
        "42" @ 67..69: Number
        "x" @ 71..72: Variable [definition]
        "MyClass" @ 75..82: Class
        "y" @ 85..86: Variable [definition]
        "my_function" @ 89..100: Function
        "z" @ 103..104: Variable [definition]
        "sys" @ 107..110: Namespace
        "version" @ 111..118: Variable
        "#);
    }

    #[test]
    fn builtin_constants() {
        let test = SemanticTokenTest::new(
            "
x = True
y = False
z = None
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 1..2: Variable [definition]
        "True" @ 5..9: BuiltinConstant
        "y" @ 10..11: Variable [definition]
        "False" @ 14..19: BuiltinConstant
        "z" @ 20..21: Variable [definition]
        "None" @ 24..28: BuiltinConstant
        "#);
    }

    #[test]
    fn builtin_constants_in_expressions() {
        let test = SemanticTokenTest::new(
            "
def check(value):
    if value is None:
        return False
    return True

result = check(None)
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "check" @ 5..10: Function [definition]
        "value" @ 11..16: Parameter [definition]
        "value" @ 26..31: Parameter
        "None" @ 35..39: BuiltinConstant
        "False" @ 56..61: BuiltinConstant
        "True" @ 73..77: BuiltinConstant
        "result" @ 79..85: Variable [definition]
        "check" @ 88..93: Function
        "None" @ 94..98: BuiltinConstant
        "#);
    }

    #[test]
    fn builtin_types() {
        let test = SemanticTokenTest::new(
            r#"
            type U = str | int

            class Test:
                a: int
                b: bool
                c: str
                d: float
                e: list[int]
                f: list[float]
                g: int | float
                h: U
            "#,
        );

        assert_snapshot!(test.to_snapshot(&test.highlight_file()), @r#"
        "U" @ 6..7: Class [definition]
        "str" @ 10..13: Class
        "int" @ 16..19: Class
        "Test" @ 27..31: Class [definition]
        "a" @ 37..38: Variable [definition]
        "int" @ 40..43: Class
        "b" @ 48..49: Variable [definition]
        "bool" @ 51..55: Class
        "c" @ 60..61: Variable [definition]
        "str" @ 63..66: Class
        "d" @ 71..72: Variable [definition]
        "float" @ 74..79: Class
        "e" @ 84..85: Variable [definition]
        "list" @ 87..91: Class
        "int" @ 92..95: Class
        "f" @ 101..102: Variable [definition]
        "list" @ 104..108: Class
        "float" @ 109..114: Class
        "g" @ 120..121: Variable [definition]
        "int" @ 123..126: Class
        "float" @ 129..134: Class
        "h" @ 139..140: Variable [definition]
        "U" @ 142..143: TypeParameter
        "#);
    }

    #[test]
    fn semantic_tokens_range() {
        let test = SemanticTokenTest::new(
            "
def function1():
    x = 42
    return x

def function2():
    y = \"hello\"
    z = True
    return y + z
",
        );

        let full_tokens = test.highlight_file();

        // Get the range that covers only the second function
        // Hardcoded offsets: function2 starts at position 42, source ends at position 108
        let range = TextRange::new(TextSize::from(42u32), TextSize::from(108u32));

        let range_tokens = test.highlight_range(range);

        // Range-based tokens should have fewer tokens than full scan
        // (should exclude tokens from function1)
        assert!(range_tokens.len() < full_tokens.len());

        // Test both full tokens and range tokens with snapshots
        assert_snapshot!(test.to_snapshot(&full_tokens), @r#"
        "function1" @ 5..14: Function [definition]
        "x" @ 22..23: Variable [definition]
        "42" @ 26..28: Number
        "x" @ 40..41: Variable
        "function2" @ 47..56: Function [definition]
        "y" @ 64..65: Variable [definition]
        "\"hello\"" @ 68..75: String
        "z" @ 80..81: Variable [definition]
        "True" @ 84..88: BuiltinConstant
        "y" @ 100..101: Variable
        "z" @ 104..105: Variable
        "#);

        assert_snapshot!(test.to_snapshot(&range_tokens), @r#"
        "function2" @ 47..56: Function [definition]
        "y" @ 64..65: Variable [definition]
        "\"hello\"" @ 68..75: String
        "z" @ 80..81: Variable [definition]
        "True" @ 84..88: BuiltinConstant
        "y" @ 100..101: Variable
        "z" @ 104..105: Variable
        "#);

        // Verify that no tokens from range_tokens have ranges outside the requested range
        for token in range_tokens.iter() {
            assert!(
                range.contains_range(token.range()),
                "Token at {:?} is outside requested range {:?}",
                token.range(),
                range
            );
        }
    }

    /// When a token starts right at where the requested range ends,
    /// don't include it in the semantic tokens.
    #[test]
    fn semantic_tokens_range_excludes_boundary_tokens() {
        let test = SemanticTokenTest::new(
            "
x = 1
y = 2
z = 3
",
        );

        // Range [6..13) starts where "1" ends and ends where "z" starts.
        // Expected: only "y" @ 7..8 and "2" @ 11..12 (non-empty overlap with target range).
        // Not included: "1" @ 5..6 and "z" @ 13..14 (adjacent, but not overlapping at offsets 6 and 13).
        let range = TextRange::new(TextSize::from(6), TextSize::from(13));

        let range_tokens = test.highlight_range(range);

        assert_snapshot!(test.to_snapshot(&range_tokens), @r#"
        "y" @ 7..8: Variable [definition]
        "2" @ 11..12: Number
        "#);
    }

    #[test]
    fn dotted_module_names() {
        let test = SemanticTokenTest::new(
            "
import os.path
import sys.version_info
from urllib.parse import urlparse
from collections.abc import Mapping
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "path" @ 11..15: Namespace
        "sys" @ 23..26: Namespace
        "version_info" @ 27..39: Namespace
        "urllib" @ 45..51: Namespace
        "parse" @ 52..57: Namespace
        "urlparse" @ 65..73: Function
        "collections" @ 79..90: Namespace
        "abc" @ 91..94: Namespace
        "Mapping" @ 102..109: Class
        "#);
    }

    #[test]
    fn module_type_classification() {
        let test = SemanticTokenTest::new(
            "
import os
import sys
from collections import defaultdict

# os and sys should be classified as namespace/module types
x = os
y = sys
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "sys" @ 18..21: Namespace
        "collections" @ 27..38: Namespace
        "defaultdict" @ 46..57: Class
        "x" @ 119..120: Variable [definition]
        "os" @ 123..125: Namespace
        "y" @ 126..127: Variable [definition]
        "sys" @ 130..133: Namespace
        "#);
    }

    #[test]
    fn import_classification() {
        let test = SemanticTokenTest::new(
            "
from os import path
from collections import defaultdict, OrderedDict, Counter
from typing import List, Dict, Optional
from mymodule import CONSTANT, my_function, MyClass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 6..8: Namespace
        "path" @ 16..20: Namespace
        "collections" @ 26..37: Namespace
        "defaultdict" @ 45..56: Class
        "OrderedDict" @ 58..69: Class
        "Counter" @ 71..78: Class
        "typing" @ 84..90: Namespace
        "List" @ 98..102: Variable
        "Dict" @ 104..108: Variable
        "Optional" @ 110..118: Variable
        "mymodule" @ 124..132: Namespace
        "#);
    }

    #[test]
    fn str_annotation() {
        let test = SemanticTokenTest::new(
            r#"
x: int = 1
y: "int" = 1
z = "int"
w1: "int | str" = "hello"
w2: "int | sr" = "hello"
w3: "int | " = "hello"
w4: "float"
w5: "float
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 1..2: Variable [definition]
        "int" @ 4..7: Class
        "1" @ 10..11: Number
        "y" @ 12..13: Variable [definition]
        "int" @ 16..19: Class
        "1" @ 23..24: Number
        "z" @ 25..26: Variable [definition]
        "\"int\"" @ 29..34: String
        "w1" @ 35..37: Variable [definition]
        "int" @ 40..43: Class
        "str" @ 46..49: Class
        "\"hello\"" @ 53..60: String
        "w2" @ 61..63: Variable [definition]
        "int" @ 66..69: Class
        "\"hello\"" @ 78..85: String
        "w3" @ 86..88: Variable [definition]
        "\"int | \"" @ 90..98: String
        "\"hello\"" @ 101..108: String
        "w4" @ 109..111: Variable [definition]
        "float" @ 114..119: Class
        "w5" @ 121..123: Variable [definition]
        "float" @ 126..131: Class
        "#);
    }

    #[test]
    fn str_annotation_nested() {
        let test = SemanticTokenTest::new(
            r#"
x: int
y: "int"
z: "'int'"
w: """'"int"'"""

a: list[int | str] | None
b: list["int | str"] | None
c: "list[int | str] | None"
d: "list[int | str]" | "None"
e: 'list["int | str"] | "None"'
f: """'list["int | str"]' | 'None'"""
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 1..2: Variable [definition]
        "int" @ 4..7: Class
        "y" @ 8..9: Variable [definition]
        "int" @ 12..15: Class
        "z" @ 17..18: Variable [definition]
        "int" @ 22..25: Class
        "w" @ 28..29: Variable [definition]
        "\"int\"" @ 35..40: String
        "a" @ 46..47: Variable [definition]
        "list" @ 49..53: Class
        "int" @ 54..57: Class
        "str" @ 60..63: Class
        "None" @ 67..71: BuiltinConstant
        "b" @ 72..73: Variable [definition]
        "list" @ 75..79: Class
        "int" @ 81..84: Class
        "str" @ 87..90: Class
        "None" @ 95..99: BuiltinConstant
        "c" @ 100..101: Variable [definition]
        "list" @ 104..108: Class
        "int" @ 109..112: Class
        "str" @ 115..118: Class
        "None" @ 122..126: BuiltinConstant
        "d" @ 128..129: Variable [definition]
        "list" @ 132..136: Class
        "int" @ 137..140: Class
        "str" @ 143..146: Class
        "None" @ 152..156: BuiltinConstant
        "e" @ 158..159: Variable [definition]
        "list" @ 162..166: Class
        "int" @ 168..171: Class
        "str" @ 174..177: Class
        "None" @ 183..187: BuiltinConstant
        "f" @ 190..191: Variable [definition]
        "list" @ 197..201: Class
        "\"int | str\"" @ 202..213: String
        "None" @ 219..223: BuiltinConstant
        "#);
    }

    #[test]
    fn attribute_classification() {
        let test = SemanticTokenTest::new(
            "
import os
import sys
from collections import defaultdict

class MyClass:
    CONSTANT = 42

    def method(self):
        return \"hello\"

    @property
    def prop(self):
        return self.CONSTANT

obj = MyClass()

# Test various attribute accesses
x = os.path              # path should be namespace (module)
y = obj.method           # method should be method (bound method)
z = obj.CONSTANT         # CONSTANT should be variable with readonly modifier
w = obj.prop             # prop should be property
v = MyClass.method       # method should be method (function)
u = MyClass.__name__     # __name__ should resolve on the class object
t = MyClass.prop          # prop should be property on the class itself
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "sys" @ 18..21: Namespace
        "collections" @ 27..38: Namespace
        "defaultdict" @ 46..57: Class
        "MyClass" @ 65..72: Class [definition]
        "CONSTANT" @ 78..86: Variable [definition, readonly]
        "42" @ 89..91: Number
        "method" @ 101..107: Method [definition]
        "self" @ 108..112: SelfParameter [definition]
        "\"hello\"" @ 130..137: String
        "property" @ 144..152: Decorator
        "prop" @ 161..165: Method [definition]
        "self" @ 166..170: SelfParameter [definition]
        "self" @ 188..192: SelfParameter
        "CONSTANT" @ 193..201: Variable [readonly]
        "obj" @ 203..206: Variable [definition]
        "MyClass" @ 209..216: Class
        "x" @ 254..255: Variable [definition]
        "os" @ 258..260: Namespace
        "path" @ 261..265: Namespace
        "y" @ 315..316: Variable [definition]
        "obj" @ 319..322: Variable
        "method" @ 323..329: Method
        "z" @ 381..382: Variable [definition]
        "obj" @ 385..388: Variable
        "CONSTANT" @ 389..397: Variable [readonly]
        "w" @ 459..460: Variable [definition]
        "obj" @ 463..466: Variable
        "prop" @ 467..471: Property [readonly]
        "v" @ 510..511: Variable [definition]
        "MyClass" @ 514..521: Class
        "method" @ 522..528: Method
        "u" @ 572..573: Variable [definition]
        "MyClass" @ 576..583: Class
        "__name__" @ 584..592: Variable
        "t" @ 643..644: Variable [definition]
        "MyClass" @ 647..654: Class
        "prop" @ 655..659: Property [readonly]
        "#);
    }

    #[test]
    fn property_with_return_annotation() {
        let test = SemanticTokenTest::new(
            "
class Foo:
    @property
    def prop(self) -> int:
        return 4

foo = Foo()
w = foo.prop
x = Foo.prop
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "Foo" @ 7..10: Class [definition]
        "property" @ 17..25: Decorator
        "prop" @ 34..38: Method [definition]
        "self" @ 39..43: SelfParameter [definition]
        "int" @ 48..51: Class
        "4" @ 68..69: Number
        "foo" @ 71..74: Variable [definition]
        "Foo" @ 77..80: Class
        "w" @ 83..84: Variable [definition]
        "foo" @ 87..90: Variable
        "prop" @ 91..95: Property [readonly]
        "x" @ 96..97: Variable [definition]
        "Foo" @ 100..103: Class
        "prop" @ 104..108: Property [readonly]
        "#);
    }

    #[test]
    fn property_readonly_modifier() {
        // Verify that the readonly modifier is set for getter-only properties
        // and NOT set for properties that also have a setter.
        let test = SemanticTokenTest::new(
            "
class Config:
    @property
    def read_only(self) -> str:
        return 'value'

    @property
    def read_write(self) -> int:
        return self._x

    @read_write.setter
    def read_write(self, value: int) -> None:
        self._x = value

cfg = Config()
a = cfg.read_only
b = cfg.read_write
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "Config" @ 7..13: Class [definition]
        "property" @ 20..28: Decorator
        "read_only" @ 37..46: Method [definition]
        "self" @ 47..51: SelfParameter [definition]
        "str" @ 56..59: Class
        "'value'" @ 76..83: String
        "property" @ 90..98: Decorator
        "read_write" @ 107..117: Method [definition]
        "self" @ 118..122: SelfParameter [definition]
        "int" @ 127..130: Class
        "self" @ 147..151: SelfParameter
        "_x" @ 152..154: Variable
        "read_write" @ 161..171: Method
        "setter" @ 172..178: Method
        "read_write" @ 187..197: Method [definition]
        "self" @ 198..202: SelfParameter [definition]
        "value" @ 204..209: Parameter [definition]
        "int" @ 211..214: Class
        "None" @ 219..223: BuiltinConstant
        "self" @ 233..237: SelfParameter
        "_x" @ 238..240: Variable
        "value" @ 243..248: Parameter
        "cfg" @ 250..253: Variable [definition]
        "Config" @ 256..262: Class
        "a" @ 265..266: Variable [definition]
        "cfg" @ 269..272: Variable
        "read_only" @ 273..282: Property [readonly]
        "b" @ 283..284: Variable [definition]
        "cfg" @ 287..290: Variable
        "read_write" @ 291..301: Property
        "#);
    }

    #[test]
    fn property_union_with_non_property_falls_back() {
        let test = SemanticTokenTest::new(
            "
class WithProperty:
    @property
    def value(self) -> int:
        return 1

class WithAttribute:
    value = 2

def f(obj: WithProperty | WithAttribute):
    return obj.value
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "WithProperty" @ 7..19: Class [definition]
        "property" @ 26..34: Decorator
        "value" @ 43..48: Method [definition]
        "self" @ 49..53: SelfParameter [definition]
        "int" @ 58..61: Class
        "1" @ 78..79: Number
        "WithAttribute" @ 87..100: Class [definition]
        "value" @ 106..111: Variable [definition]
        "2" @ 114..115: Number
        "f" @ 121..122: Function [definition]
        "obj" @ 123..126: Parameter [definition]
        "WithProperty" @ 128..140: Class
        "WithAttribute" @ 143..156: Class
        "obj" @ 170..173: Parameter
        "value" @ 174..179: Variable
        "#);
    }

    #[test]
    fn property_union_readonly_only_if_all_variants_are_readonly() {
        let test = SemanticTokenTest::new(
            "
from random import random

class ReadOnly:
    @property
    def value(self) -> int:
        return 1

class ReadWrite:
    @property
    def value(self) -> int:
        return self._value

    @value.setter
    def value(self, new_value: int) -> None:
        self._value = new_value

obj = ReadOnly() if random() else ReadWrite()
x = obj.value
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "random" @ 6..12: Namespace
        "random" @ 20..26: Method
        "ReadOnly" @ 34..42: Class [definition]
        "property" @ 49..57: Decorator
        "value" @ 66..71: Method [definition]
        "self" @ 72..76: SelfParameter [definition]
        "int" @ 81..84: Class
        "1" @ 101..102: Number
        "ReadWrite" @ 110..119: Class [definition]
        "property" @ 126..134: Decorator
        "value" @ 143..148: Method [definition]
        "self" @ 149..153: SelfParameter [definition]
        "int" @ 158..161: Class
        "self" @ 178..182: SelfParameter
        "_value" @ 183..189: Variable
        "value" @ 196..201: Method
        "setter" @ 202..208: Method
        "value" @ 217..222: Method [definition]
        "self" @ 223..227: SelfParameter [definition]
        "new_value" @ 229..238: Parameter [definition]
        "int" @ 240..243: Class
        "None" @ 248..252: BuiltinConstant
        "self" @ 262..266: SelfParameter
        "_value" @ 267..273: Variable
        "new_value" @ 276..285: Parameter
        "obj" @ 287..290: Variable [definition]
        "ReadOnly" @ 293..301: Class
        "random" @ 307..313: Variable
        "ReadWrite" @ 321..330: Class
        "x" @ 333..334: Variable [definition]
        "obj" @ 337..340: Variable
        "value" @ 341..346: Property
        "#);
    }

    #[test]
    fn attribute_fallback_classification() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    some_attr = \"value\"

obj = MyClass()
# Test attribute that might not have detailed semantic info
x = obj.some_attr        # Should fall back to variable, not property
y = obj.unknown_attr     # Should fall back to variable
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "some_attr" @ 20..29: Variable [definition]
        "\"value\"" @ 32..39: String
        "obj" @ 41..44: Variable [definition]
        "MyClass" @ 47..54: Class
        "x" @ 117..118: Variable [definition]
        "obj" @ 121..124: Variable
        "some_attr" @ 125..134: Variable
        "y" @ 187..188: Variable [definition]
        "obj" @ 191..194: Variable
        "#);
    }

    #[test]
    fn attribute_on_union_1() {
        let test = SemanticTokenTest::new(
            "
from random import random

class Foo:
    CONSTANT = 42

    def method(self):
        return \"hello\"

    @property
    def prop(self) -> str:
        return \"hello\"

class Bar:
    CONSTANT = 24

    def method(self, x: int = 1) -> int:
        return 42

    @property
    def prop(self) -> int:
        return self.CONSTANT


foobar = Foo() if random() else Bar()
y = foobar.method                                # method should be method (bound method)
z = foobar.CONSTANT                              # CONSTANT should be variable with readonly modifier
w = foobar.prop                                  # prop should be property
foobar_cls = Foo if random() else Bar
v = foobar_cls.method                            # method should be method (function)
x = foobar_cls.prop                              # prop should be property
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "random" @ 6..12: Namespace
        "random" @ 20..26: Method
        "Foo" @ 34..37: Class [definition]
        "CONSTANT" @ 43..51: Variable [definition, readonly]
        "42" @ 54..56: Number
        "method" @ 66..72: Method [definition]
        "self" @ 73..77: SelfParameter [definition]
        "\"hello\"" @ 95..102: String
        "property" @ 109..117: Decorator
        "prop" @ 126..130: Method [definition]
        "self" @ 131..135: SelfParameter [definition]
        "str" @ 140..143: Class
        "\"hello\"" @ 160..167: String
        "Bar" @ 175..178: Class [definition]
        "CONSTANT" @ 184..192: Variable [definition, readonly]
        "24" @ 195..197: Number
        "method" @ 207..213: Method [definition]
        "self" @ 214..218: SelfParameter [definition]
        "x" @ 220..221: Parameter [definition]
        "int" @ 223..226: Class
        "1" @ 229..230: Number
        "int" @ 235..238: Class
        "42" @ 255..257: Number
        "property" @ 264..272: Decorator
        "prop" @ 281..285: Method [definition]
        "self" @ 286..290: SelfParameter [definition]
        "int" @ 295..298: Class
        "self" @ 315..319: SelfParameter
        "CONSTANT" @ 320..328: Variable [readonly]
        "foobar" @ 331..337: Variable [definition]
        "Foo" @ 340..343: Class
        "random" @ 349..355: Variable
        "Bar" @ 363..366: Class
        "y" @ 369..370: Variable [definition]
        "foobar" @ 373..379: Variable
        "method" @ 380..386: Method
        "z" @ 459..460: Variable [definition]
        "foobar" @ 463..469: Variable
        "CONSTANT" @ 470..478: Variable [readonly]
        "w" @ 561..562: Variable [definition]
        "foobar" @ 565..571: Variable
        "prop" @ 572..576: Property [readonly]
        "foobar_cls" @ 636..646: Variable [definition]
        "Foo" @ 649..652: Class
        "random" @ 656..662: Variable
        "Bar" @ 670..673: Class
        "v" @ 674..675: Variable [definition]
        "foobar_cls" @ 678..688: Variable
        "method" @ 689..695: Method
        "x" @ 760..761: Variable [definition]
        "foobar_cls" @ 764..774: Variable
        "prop" @ 775..779: Property [readonly]
        "#);
    }

    #[test]
    fn attribute_on_union_2() {
        let test = SemanticTokenTest::new(
            "
from random import random

# There is also this way to create union types:
class Baz:
    if random():
        CONSTANT = 42

        def method(self) -> int:
            return 42

        @property
        def prop(self) -> int:
            return 42
    else:
        CONSTANT = \"hello\"

        def method(self) -> str:
            return \"hello\"

        @property
        def prop(self) -> str:
            return \"hello\"

baz = Baz()
s = baz.method      # method should be bound method
t = baz.CONSTANT    # CONSTANT should be variable with readonly
r = baz.prop        # prop should be property
q = Baz.prop        # prop should be property on the class as well
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "random" @ 6..12: Namespace
        "random" @ 20..26: Method
        "Baz" @ 82..85: Class [definition]
        "random" @ 94..100: Variable
        "CONSTANT" @ 112..120: Variable [definition, readonly]
        "42" @ 123..125: Number
        "method" @ 139..145: Method [definition]
        "self" @ 146..150: SelfParameter [definition]
        "int" @ 155..158: Class
        "42" @ 179..181: Number
        "property" @ 192..200: Decorator
        "prop" @ 213..217: Method [definition]
        "self" @ 218..222: SelfParameter [definition]
        "int" @ 227..230: Class
        "42" @ 251..253: Number
        "CONSTANT" @ 272..280: Variable [definition, readonly]
        "\"hello\"" @ 283..290: String
        "method" @ 304..310: Method [definition]
        "self" @ 311..315: SelfParameter [definition]
        "str" @ 320..323: Class
        "\"hello\"" @ 344..351: String
        "property" @ 362..370: Decorator
        "prop" @ 383..387: Method [definition]
        "self" @ 388..392: SelfParameter [definition]
        "str" @ 397..400: Class
        "\"hello\"" @ 421..428: String
        "baz" @ 430..433: Variable [definition]
        "Baz" @ 436..439: Class
        "s" @ 442..443: Variable [definition]
        "baz" @ 446..449: Variable
        "method" @ 450..456: Method
        "t" @ 494..495: Variable [definition]
        "baz" @ 498..501: Variable
        "CONSTANT" @ 502..510: Variable [readonly]
        "r" @ 558..559: Variable [definition]
        "baz" @ 562..565: Variable
        "prop" @ 566..570: Property [readonly]
        "q" @ 604..605: Variable [definition]
        "Baz" @ 608..611: Class
        "prop" @ 612..616: Property [readonly]
        "#);
    }

    #[test]
    fn attribute_on_union_3() {
        // This is a test where the unions are not actually composed of the same elements,
        // so the regular fallback logic should apply.
        let test = SemanticTokenTest::new(
            "
from random import random

class Baz:
    if random():
        CONSTANT = 42

        def method(self) -> int:
            return 42

        @property
        def prop(self) -> int:
            return 42
    else:
        def CONSTANT(self):
            return \"hello\"

        @property
        def method(self) -> str:
            return \"hello\"

        prop: str = \"hello\"

baz = Baz()
s = baz.method
t = baz.CONSTANT
r = baz.prop
q = Baz.prop
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "random" @ 6..12: Namespace
        "random" @ 20..26: Method
        "Baz" @ 34..37: Class [definition]
        "random" @ 46..52: Variable
        "CONSTANT" @ 64..72: Variable [definition, readonly]
        "42" @ 75..77: Number
        "method" @ 91..97: Method [definition]
        "self" @ 98..102: SelfParameter [definition]
        "int" @ 107..110: Class
        "42" @ 131..133: Number
        "property" @ 144..152: Decorator
        "prop" @ 165..169: Method [definition]
        "self" @ 170..174: SelfParameter [definition]
        "int" @ 179..182: Class
        "42" @ 203..205: Number
        "CONSTANT" @ 228..236: Method [definition]
        "self" @ 237..241: SelfParameter [definition]
        "\"hello\"" @ 263..270: String
        "property" @ 281..289: Decorator
        "method" @ 302..308: Method [definition]
        "self" @ 309..313: SelfParameter [definition]
        "str" @ 318..321: Class
        "\"hello\"" @ 342..349: String
        "prop" @ 359..363: Method [definition]
        "str" @ 365..368: Class
        "\"hello\"" @ 371..378: String
        "baz" @ 380..383: Variable [definition]
        "Baz" @ 386..389: Class
        "s" @ 392..393: Variable [definition]
        "baz" @ 396..399: Variable
        "method" @ 400..406: Variable
        "t" @ 407..408: Variable [definition]
        "baz" @ 411..414: Variable
        "CONSTANT" @ 415..423: Variable [readonly]
        "r" @ 424..425: Variable [definition]
        "baz" @ 428..431: Variable
        "prop" @ 432..436: Variable
        "q" @ 437..438: Variable [definition]
        "Baz" @ 441..444: Class
        "prop" @ 445..449: Variable
        "#);
    }

    #[test]
    fn constant_name_detection() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    UPPER_CASE = 42
    lower_case = 24
    MixedCase = 12
    A = 1

obj = MyClass()
x = obj.UPPER_CASE    # Should have readonly modifier
y = obj.lower_case    # Should not have readonly modifier
z = obj.MixedCase     # Should not have readonly modifier
w = obj.A             # Should not have readonly modifier (length == 1)
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "UPPER_CASE" @ 20..30: Variable [definition, readonly]
        "42" @ 33..35: Number
        "lower_case" @ 40..50: Variable [definition]
        "24" @ 53..55: Number
        "MixedCase" @ 60..69: Variable [definition]
        "12" @ 72..74: Number
        "A" @ 79..80: Variable [definition]
        "1" @ 83..84: Number
        "obj" @ 86..89: Variable [definition]
        "MyClass" @ 92..99: Class
        "x" @ 102..103: Variable [definition]
        "obj" @ 106..109: Variable
        "UPPER_CASE" @ 110..120: Variable [readonly]
        "y" @ 156..157: Variable [definition]
        "obj" @ 160..163: Variable
        "lower_case" @ 164..174: Variable
        "z" @ 214..215: Variable [definition]
        "obj" @ 218..221: Variable
        "MixedCase" @ 222..231: Variable
        "w" @ 272..273: Variable [definition]
        "obj" @ 276..279: Variable
        "A" @ 280..281: Variable
        "#);
    }

    #[test]
    fn type_annotations() {
        let test = SemanticTokenTest::new(
            r#"
from typing import List, Optional

def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
    pass

x: int = 42
y: Optional[str] = None
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "List" @ 20..24: Variable
        "Optional" @ 26..34: Variable
        "function_with_annotations" @ 40..65: Function [definition]
        "param1" @ 66..72: Parameter [definition]
        "int" @ 74..77: Class
        "param2" @ 79..85: Parameter [definition]
        "str" @ 87..90: Class
        "Optional" @ 95..103: Variable
        "List" @ 104..108: Variable
        "str" @ 109..112: Class
        "x" @ 126..127: Variable [definition]
        "int" @ 129..132: Class
        "42" @ 135..137: Number
        "y" @ 138..139: Variable [definition]
        "Optional" @ 141..149: Variable
        "str" @ 150..153: Class
        "None" @ 157..161: BuiltinConstant
        "#);
    }

    #[test]
    fn type_alias_values_use_type_form_highlighting() {
        let test = SemanticTokenTest::new(
            r#"
from typing import IO, TypeAlias

def takes_file(x: IO[str]) -> None: ...

type NewStyle = IO[str]
LegacyStyle: TypeAlias = IO[str]
"#,
        );

        let tokens = test.highlight_file();
        let source = ruff_db::source::source_text(&test.db, test.file);
        let io_ranges: Vec<_> = source
            .match_indices("IO")
            .skip(1)
            .map(|(offset, _)| {
                TextRange::at(
                    TextSize::from(
                        u32::try_from(offset).expect("source offset to fit into TextSize"),
                    ),
                    "IO".text_len(),
                )
            })
            .collect();

        assert_eq!(
            io_ranges.len(),
            3,
            "expected annotation and alias RHS `IO` uses"
        );

        for io_range in io_ranges {
            let token = tokens
                .iter()
                .find(|token| token.range == io_range)
                .expect("semantic token for `IO` type-form use");
            assert_eq!(token.token_type, SemanticTokenType::Class);
        }
    }

    #[test]
    fn generic_class_members_in_annotations() {
        let test = SemanticTokenTest::new(
            r#"
import os
from os import PathLike

x1: os.PathLike
x2: os.PathLike[str]

y1: PathLike
y2: PathLike[str]

z1 = os.PathLike
z2 = os.PathLike[str]
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "os" @ 16..18: Namespace
        "PathLike" @ 26..34: Class
        "x1" @ 36..38: Variable [definition]
        "os" @ 40..42: Namespace
        "PathLike" @ 43..51: Class
        "x2" @ 52..54: Variable [definition]
        "os" @ 56..58: Namespace
        "PathLike" @ 59..67: Class
        "str" @ 68..71: Class
        "y1" @ 74..76: Variable [definition]
        "PathLike" @ 78..86: Class
        "y2" @ 87..89: Variable [definition]
        "PathLike" @ 91..99: Class
        "str" @ 100..103: Class
        "z1" @ 106..108: Class [definition]
        "os" @ 111..113: Namespace
        "PathLike" @ 114..122: Class
        "z2" @ 123..125: Class [definition]
        "os" @ 128..130: Namespace
        "PathLike" @ 131..139: Class
        "str" @ 140..143: Class
        "#);
    }

    #[test]
    fn generic_class_members_in_cast() {
        let test = SemanticTokenTest::new(
            r#"
import os
import typing
from os import PathLike
from typing import cast

x1 = cast(os.PathLike[str], "")
x2 = cast(PathLike[str], "")
x3 = typing.cast(os.PathLike[str], "")
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "typing" @ 18..24: Namespace
        "os" @ 30..32: Namespace
        "PathLike" @ 40..48: Class
        "typing" @ 54..60: Namespace
        "cast" @ 68..72: Function
        "x1" @ 74..76: Variable [definition]
        "cast" @ 79..83: Function
        "os" @ 84..86: Namespace
        "PathLike" @ 87..95: Class
        "str" @ 96..99: Class
        "\"\"" @ 102..104: String
        "x2" @ 106..108: Variable [definition]
        "cast" @ 111..115: Function
        "PathLike" @ 116..124: Class
        "str" @ 125..128: Class
        "\"\"" @ 131..133: String
        "x3" @ 135..137: Variable [definition]
        "typing" @ 140..146: Namespace
        "cast" @ 147..151: Method
        "os" @ 152..154: Namespace
        "PathLike" @ 155..163: Class
        "str" @ 164..167: Class
        "\"\"" @ 170..172: String
        "#);
    }

    #[test]
    fn generic_class_members_in_assert_type() {
        let test = SemanticTokenTest::new(
            r#"
import os
import typing
from os import PathLike
from typing import assert_type

x1 = assert_type("", os.PathLike[str])
x2 = assert_type("", PathLike[str])
x3 = typing.assert_type("", os.PathLike[str])
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "typing" @ 18..24: Namespace
        "os" @ 30..32: Namespace
        "PathLike" @ 40..48: Class
        "typing" @ 54..60: Namespace
        "assert_type" @ 68..79: Function
        "x1" @ 81..83: Variable [definition]
        "assert_type" @ 86..97: Function
        "\"\"" @ 98..100: String
        "os" @ 102..104: Namespace
        "PathLike" @ 105..113: Class
        "str" @ 114..117: Class
        "x2" @ 120..122: Variable [definition]
        "assert_type" @ 125..136: Function
        "\"\"" @ 137..139: String
        "PathLike" @ 141..149: Class
        "str" @ 150..153: Class
        "x3" @ 156..158: Variable [definition]
        "typing" @ 161..167: Namespace
        "assert_type" @ 168..179: Method
        "\"\"" @ 180..182: String
        "os" @ 184..186: Namespace
        "PathLike" @ 187..195: Class
        "str" @ 196..199: Class
        "#);
    }

    #[test]
    fn generic_class_members_in_type_form_keyword_arguments() {
        let test = SemanticTokenTest::new(
            r#"
import os
from os import PathLike
from typing import assert_type, cast

x1 = cast(typ=os.PathLike[str], val="")
x2 = cast(val="", typ=PathLike[str])
x3 = assert_type(type=os.PathLike[str], value="")
x4 = assert_type(value="", type=PathLike[str])
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "os" @ 16..18: Namespace
        "PathLike" @ 26..34: Class
        "typing" @ 40..46: Namespace
        "assert_type" @ 54..65: Function
        "cast" @ 67..71: Function
        "x1" @ 73..75: Variable [definition]
        "cast" @ 78..82: Function
        "os" @ 87..89: Namespace
        "PathLike" @ 90..98: Class
        "str" @ 99..102: Class
        "\"\"" @ 109..111: String
        "x2" @ 113..115: Variable [definition]
        "cast" @ 118..122: Function
        "\"\"" @ 127..129: String
        "PathLike" @ 135..143: Class
        "str" @ 144..147: Class
        "x3" @ 150..152: Variable [definition]
        "assert_type" @ 155..166: Function
        "os" @ 172..174: Namespace
        "PathLike" @ 175..183: Class
        "str" @ 184..187: Class
        "\"\"" @ 196..198: String
        "x4" @ 200..202: Variable [definition]
        "assert_type" @ 205..216: Function
        "\"\"" @ 223..225: String
        "PathLike" @ 232..240: Class
        "str" @ 241..244: Class
        "#);
    }

    #[test]
    fn semantic_tokens_ignore_failed_bindings_for_type_form_arguments() {
        let test = SemanticTokenTest::new(
            r#"
from typing import cast

flag = bool(input())
def g(x):
    return x

x = ""
f = cast if flag else g
f(int, x)
"#,
        );

        let tokens = test.highlight_file();
        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "cast" @ 20..24: Function
        "flag" @ 26..30: Variable [definition]
        "bool" @ 33..37: Class
        "input" @ 38..43: Function
        "g" @ 51..52: Function [definition]
        "x" @ 53..54: Parameter [definition]
        "x" @ 68..69: Parameter
        "x" @ 71..72: Variable [definition]
        "\"\"" @ 75..77: String
        "f" @ 78..79: Variable [definition]
        "cast" @ 82..86: Function
        "flag" @ 90..94: Variable
        "g" @ 100..101: Function
        "f" @ 102..103: Variable
        "int" @ 104..107: Class
        "x" @ 109..110: Variable
        "#);
    }

    #[test]
    fn function_docstring_classification() {
        let test = SemanticTokenTest::new(
            r#"
def my_function(param1: int, param2: str) -> bool:
    """Example function

    Args:
        param1: The first parameter.
        param2: The second parameter.

    Returns:
        The return value. True for success, False otherwise.

    """

    x = "hello"
    def other_func(): pass

    """unrelated string"""

    return False
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "my_function" @ 5..16: Function [definition]
        "param1" @ 17..23: Parameter [definition]
        "int" @ 25..28: Class
        "param2" @ 30..36: Parameter [definition]
        "str" @ 38..41: Class
        "bool" @ 46..50: Class
        "\"\"\"Example function\n\n    Args:\n        param1: The first parameter.\n        param2: The second parameter.\n\n    Returns:\n        The return value. True for success, False otherwise.\n\n    \"\"\"" @ 56..245: String [documentation]
        "x" @ 251..252: Variable [definition]
        "\"hello\"" @ 255..262: String
        "other_func" @ 271..281: Function [definition]
        "\"\"\"unrelated string\"\"\"" @ 295..317: String
        "False" @ 330..335: BuiltinConstant
        "#);
    }

    #[test]
    fn class_docstring_classification() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    """Example class

    What a good class wowwee
    """

    def __init__(self): pass

    """unrelated string"""

    x: str = "hello"
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"\"\"Example class\n\n    What a good class wowwee\n    \"\"\"" @ 20..74: String [documentation]
        "__init__" @ 84..92: Method [definition]
        "self" @ 93..97: SelfParameter [definition]
        "\"\"\"unrelated string\"\"\"" @ 110..132: String
        "x" @ 138..139: Variable [definition]
        "str" @ 141..144: Class
        "\"hello\"" @ 147..154: String
        "#);
    }

    #[test]
    fn module_docstring_classification() {
        let test = SemanticTokenTest::new(
            r#"
"""Example module

What a good module wooo
"""

def my_func(): pass

"""unrelated string"""

x: str = "hello"
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "\"\"\"Example module\n\nWhat a good module wooo\n\"\"\"" @ 1..47: String [documentation]
        "my_func" @ 53..60: Function [definition]
        "\"\"\"unrelated string\"\"\"" @ 70..92: String
        "x" @ 94..95: Variable [definition]
        "str" @ 97..100: Class
        "\"hello\"" @ 103..110: String
        "#);
    }

    #[test]
    fn attribute_docstring_classification() {
        let test = SemanticTokenTest::new(
            r#"
important_value: str = "wow"
"""This is the most important value

Don't trust the other guy
"""

x = "unrelated string"

other_value: int = 2
"""This is such an import value omg

Trust me
"""
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "important_value" @ 1..16: Variable [definition]
        "str" @ 18..21: Class
        "\"wow\"" @ 24..29: String
        "\"\"\"This is the most important value\n\nDon't trust the other guy\n\"\"\"" @ 30..96: String [documentation]
        "x" @ 98..99: Variable [definition]
        "\"unrelated string\"" @ 102..120: String
        "other_value" @ 122..133: Variable [definition]
        "int" @ 135..138: Class
        "2" @ 141..142: Number
        "\"\"\"This is such an import value omg\n\nTrust me\n\"\"\"" @ 143..192: String [documentation]
        "#);
    }

    #[test]
    fn attribute_docstring_classification_spill() {
        let test = SemanticTokenTest::new(
            r#"
if True:
    x = 1
"this shouldn't be a docstring but also it doesn't matter much"
"""
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "True" @ 4..8: BuiltinConstant
        "x" @ 14..15: Variable [definition]
        "1" @ 18..19: Number
        "\"this shouldn't be a docstring but also it doesn't matter much\"" @ 20..83: String [documentation]
        "\"\"\"\n" @ 84..88: String
        "#);
    }

    #[test]
    fn docstring_classification_concat() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    """wow cool docs""" """and docs"""

def my_func():
    """wow cool docs""" """and docs"""

x = 1
"""wow cool docs""" """and docs"""
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"\"\"wow cool docs\"\"\"" @ 20..39: String [documentation]
        "\"\"\"and docs\"\"\"" @ 40..54: String [documentation]
        "my_func" @ 60..67: Function [definition]
        "\"\"\"wow cool docs\"\"\"" @ 75..94: String [documentation]
        "\"\"\"and docs\"\"\"" @ 95..109: String [documentation]
        "x" @ 111..112: Variable [definition]
        "1" @ 115..116: Number
        "\"\"\"wow cool docs\"\"\"" @ 117..136: String [documentation]
        "\"\"\"and docs\"\"\"" @ 137..151: String [documentation]
        "#);
    }

    #[test]
    fn docstring_classification_concat_parens() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    (
        """wow cool docs"""
        """and docs"""
    )

def my_func():
    (
        """wow cool docs"""
        """and docs"""
    )

x = 1
(
    """wow cool docs"""
    """and docs"""
)
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"\"\"wow cool docs\"\"\"" @ 30..49: String [documentation]
        "\"\"\"and docs\"\"\"" @ 58..72: String [documentation]
        "my_func" @ 84..91: Function [definition]
        "\"\"\"wow cool docs\"\"\"" @ 109..128: String [documentation]
        "\"\"\"and docs\"\"\"" @ 137..151: String [documentation]
        "x" @ 159..160: Variable [definition]
        "1" @ 163..164: Number
        "\"\"\"wow cool docs\"\"\"" @ 171..190: String [documentation]
        "\"\"\"and docs\"\"\"" @ 195..209: String [documentation]
        "#);
    }

    #[test]
    fn docstring_classification_concat_parens_commented_nextline() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    (
        """wow cool docs"""
        # and a comment that shouldn't be included
        """and docs"""
    )

def my_func():
    (
        """wow cool docs"""
        # and a comment that shouldn't be included
        """and docs"""
    )

x = 1
(
    """wow cool docs"""
    # and a comment that shouldn't be included
    """and docs"""
)
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"\"\"wow cool docs\"\"\"" @ 30..49: String [documentation]
        "\"\"\"and docs\"\"\"" @ 109..123: String [documentation]
        "my_func" @ 135..142: Function [definition]
        "\"\"\"wow cool docs\"\"\"" @ 160..179: String [documentation]
        "\"\"\"and docs\"\"\"" @ 239..253: String [documentation]
        "x" @ 261..262: Variable [definition]
        "1" @ 265..266: Number
        "\"\"\"wow cool docs\"\"\"" @ 273..292: String [documentation]
        "\"\"\"and docs\"\"\"" @ 344..358: String [documentation]
        "#);
    }

    #[test]
    fn docstring_classification_concat_commented_nextline() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    """wow cool docs"""
    # and a comment that shouldn't be included
    """and docs"""

def my_func():
    """wow cool docs"""
    # and a comment that shouldn't be included
    """and docs"""

x = 1
"""wow cool docs"""
# and a comment that shouldn't be included
"""and docs"""
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"\"\"wow cool docs\"\"\"" @ 20..39: String [documentation]
        "\"\"\"and docs\"\"\"" @ 91..105: String
        "my_func" @ 111..118: Function [definition]
        "\"\"\"wow cool docs\"\"\"" @ 126..145: String [documentation]
        "\"\"\"and docs\"\"\"" @ 197..211: String
        "x" @ 213..214: Variable [definition]
        "1" @ 217..218: Number
        "\"\"\"wow cool docs\"\"\"" @ 219..238: String [documentation]
        "\"\"\"and docs\"\"\"" @ 282..296: String
        "#);
    }

    #[test]
    fn docstring_classification_concat_commented_sameline() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    """wow cool docs""" # and a comment
    """and docs"""      # that shouldn't be included

def my_func():
    """wow cool docs""" # and a comment
    """and docs"""      # that shouldn't be included

x = 1
"""wow cool docs""" # and a comment
"""and docs"""      # that shouldn't be included
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"\"\"wow cool docs\"\"\"" @ 20..39: String [documentation]
        "\"\"\"and docs\"\"\"" @ 60..74: String
        "my_func" @ 114..121: Function [definition]
        "\"\"\"wow cool docs\"\"\"" @ 129..148: String [documentation]
        "\"\"\"and docs\"\"\"" @ 169..183: String
        "x" @ 219..220: Variable [definition]
        "1" @ 223..224: Number
        "\"\"\"wow cool docs\"\"\"" @ 225..244: String [documentation]
        "\"\"\"and docs\"\"\"" @ 261..275: String
        "#);
    }

    #[test]
    fn docstring_classification_concat_slashed() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    """wow cool docs""" \
    """and docs"""

def my_func():
    """wow cool docs""" \
    """and docs"""

x = 1
"""wow cool docs""" \
"""and docs"""
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"\"\"wow cool docs\"\"\"" @ 20..39: String [documentation]
        "\"\"\"and docs\"\"\"" @ 46..60: String [documentation]
        "my_func" @ 66..73: Function [definition]
        "\"\"\"wow cool docs\"\"\"" @ 81..100: String [documentation]
        "\"\"\"and docs\"\"\"" @ 107..121: String [documentation]
        "x" @ 123..124: Variable [definition]
        "1" @ 127..128: Number
        "\"\"\"wow cool docs\"\"\"" @ 129..148: String [documentation]
        "\"\"\"and docs\"\"\"" @ 151..165: String [documentation]
        "#);
    }

    #[test]
    fn docstring_classification_plus() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    "wow cool docs" + "and docs"

def my_func():
    "wow cool docs" + "and docs"

x = 1
"wow cool docs" + "and docs"
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "\"wow cool docs\"" @ 20..35: String
        "\"and docs\"" @ 38..48: String
        "my_func" @ 54..61: Function [definition]
        "\"wow cool docs\"" @ 69..84: String
        "\"and docs\"" @ 87..97: String
        "x" @ 99..100: Variable [definition]
        "1" @ 103..104: Number
        "\"wow cool docs\"" @ 105..120: String
        "\"and docs\"" @ 123..133: String
        "#);
    }

    #[test]
    fn class_attribute_docstring_classification() {
        let test = SemanticTokenTest::new(
            r#"
class MyClass:
    important_value: str = "wow"
    """This is the most important value

    Don't trust the other guy
    """

    x = "unrelated string"

    other_value: int = 2
    """This is such an import value omg

    Trust me
    """
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "important_value" @ 20..35: Variable [definition]
        "str" @ 37..40: Class
        "\"wow\"" @ 43..48: String
        "\"\"\"This is the most important value\n\n    Don't trust the other guy\n    \"\"\"" @ 53..127: String [documentation]
        "x" @ 133..134: Variable [definition]
        "\"unrelated string\"" @ 137..155: String
        "other_value" @ 161..172: Variable [definition]
        "int" @ 174..177: Class
        "2" @ 180..181: Number
        "\"\"\"This is such an import value omg\n\n    Trust me\n    \"\"\"" @ 186..243: String [documentation]
        "#);
    }

    #[test]
    fn debug_int_classification() {
        let test = SemanticTokenTest::new(
            "
x: int = 42
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 1..2: Variable [definition]
        "int" @ 4..7: Class
        "42" @ 10..12: Number
        "#);
    }

    #[test]
    fn debug_user_defined_type_classification() {
        let test = SemanticTokenTest::new(
            "
class MyClass:
    pass

x: MyClass = MyClass()
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "x" @ 26..27: Variable [definition]
        "MyClass" @ 29..36: Class
        "MyClass" @ 39..46: Class
        "#);
    }

    #[test]
    fn type_annotation_vs_variable_classification() {
        let test = SemanticTokenTest::new(
            "
from typing import List, Optional

class MyClass:
    pass

def test_function(param: int, other: MyClass) -> Optional[List[str]]:
    # Variable assignments - should be Variable tokens
    x: int = 42
    y: MyClass = MyClass()
    z: List[str] = [\"hello\"]

    # Type annotations should be Class tokens:
    # int, MyClass, Optional, List, str
    return None
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "List" @ 20..24: Variable
        "Optional" @ 26..34: Variable
        "MyClass" @ 42..49: Class [definition]
        "test_function" @ 65..78: Function [definition]
        "param" @ 79..84: Parameter [definition]
        "int" @ 86..89: Class
        "other" @ 91..96: Parameter [definition]
        "MyClass" @ 98..105: Class
        "Optional" @ 110..118: Variable
        "List" @ 119..123: Variable
        "str" @ 124..127: Class
        "x" @ 190..191: Variable [definition]
        "int" @ 193..196: Class
        "42" @ 199..201: Number
        "y" @ 206..207: Variable [definition]
        "MyClass" @ 209..216: Class
        "MyClass" @ 219..226: Class
        "z" @ 233..234: Variable [definition]
        "List" @ 236..240: Variable
        "str" @ 241..244: Class
        "\"hello\"" @ 249..256: String
        "None" @ 357..361: BuiltinConstant
        "#);
    }

    #[test]
    fn protocol_types_in_annotations() {
        let test = SemanticTokenTest::new(
            "
from typing import Protocol

class MyProtocol(Protocol):
    def method(self) -> int: ...

def test_function(param: MyProtocol) -> None:
    pass
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "Protocol" @ 20..28: Variable
        "MyProtocol" @ 36..46: Class [definition]
        "Protocol" @ 47..55: Variable
        "method" @ 66..72: Method [definition]
        "self" @ 73..77: SelfParameter [definition]
        "int" @ 82..85: Class
        "test_function" @ 96..109: Function [definition]
        "param" @ 110..115: Parameter [definition]
        "MyProtocol" @ 117..127: Class
        "None" @ 132..136: BuiltinConstant
        "#);
    }

    #[test]
    fn protocol_type_annotation_vs_value_context() {
        let test = SemanticTokenTest::new(
            "
from typing import Protocol

class MyProtocol(Protocol):
    def method(self) -> int: ...

# Value context - MyProtocol is still a class literal, so should be Class
my_protocol_var = MyProtocol

# Type annotation context - should be Class
def test_function(param: MyProtocol) -> MyProtocol:
    return param
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "Protocol" @ 20..28: Variable
        "MyProtocol" @ 36..46: Class [definition]
        "Protocol" @ 47..55: Variable
        "method" @ 66..72: Method [definition]
        "self" @ 73..77: SelfParameter [definition]
        "int" @ 82..85: Class
        "my_protocol_var" @ 166..181: Class [definition]
        "MyProtocol" @ 184..194: Class
        "test_function" @ 244..257: Function [definition]
        "param" @ 258..263: Parameter [definition]
        "MyProtocol" @ 265..275: Class
        "MyProtocol" @ 280..290: Class
        "param" @ 303..308: Parameter
        "#);
    }

    #[test]
    fn type_alias_type_of() {
        let test = SemanticTokenTest::new(
            "
class Test[T]: ...

my_type_alias = Test[str]  # TODO: `my_type_alias` should be classified as a Class

def test_function(param: my_type_alias): ...
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "Test" @ 7..11: Class [definition]
        "T" @ 12..13: TypeParameter [definition]
        "my_type_alias" @ 21..34: Class [definition]
        "Test" @ 37..41: Class
        "str" @ 42..45: Class
        "test_function" @ 109..122: Function [definition]
        "param" @ 123..128: Parameter [definition]
        "my_type_alias" @ 130..143: Class
        "#);
    }

    #[test]
    fn type_alias_to_generic_alias() {
        let test = SemanticTokenTest::new(
            "
my_type_alias = type[str]

def test_function(param: my_type_alias): ...
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "my_type_alias" @ 1..14: Variable [definition]
        "type" @ 17..21: Class
        "str" @ 22..25: Class
        "test_function" @ 32..45: Function [definition]
        "param" @ 46..51: Parameter [definition]
        "my_type_alias" @ 53..66: Variable
        "#);
    }

    #[test]
    fn type_parameters_pep695() {
        let test = SemanticTokenTest::new(
            "
# Test Python 3.12 PEP 695 type parameter syntax

# Generic function with TypeVar
def func[T](x: T) -> T:
    return x

# Generic function with TypeVarTuple
def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
    return args

# Generic function with ParamSpec
def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> str:
        return str(func(*args, **kwargs))
    return wrapper

# Generic class with multiple type parameters
class Container[T, U]:
    def __init__(self, value1: T, value2: U):
        self.value1: T = value1
        self.value2: U = value2

    def get_first(self) -> T:
        return self.value1

    def get_second(self) -> U:
        return self.value2

# Generic class with bounds and defaults
class BoundedContainer[T: int, U = str]:
    def process(self, x: T, y: U) -> tuple[T, U]:
        return (x, y)
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "func" @ 87..91: Function [definition]
        "T" @ 92..93: TypeParameter [definition]
        "x" @ 95..96: Parameter [definition]
        "T" @ 98..99: TypeParameter
        "T" @ 104..105: TypeParameter
        "x" @ 118..119: Parameter
        "func_tuple" @ 162..172: Function [definition]
        "Ts" @ 174..176: TypeParameter [definition]
        "args" @ 178..182: Parameter [definition]
        "tuple" @ 184..189: Class
        "Ts" @ 191..193: Variable
        "tuple" @ 199..204: Class
        "Ts" @ 206..208: Variable
        "args" @ 222..226: Parameter
        "func_paramspec" @ 266..280: Function [definition]
        "P" @ 283..284: TypeParameter [definition]
        "func" @ 286..290: Parameter [definition]
        "P" @ 301..302: TypeParameter
        "int" @ 304..307: Class
        "P" @ 322..323: TypeParameter
        "str" @ 325..328: Class
        "wrapper" @ 339..346: Function [definition]
        "args" @ 348..352: Parameter [definition]
        "P" @ 354..355: TypeParameter
        "args" @ 356..360: Property [readonly]
        "kwargs" @ 364..370: Parameter [definition]
        "P" @ 372..373: TypeParameter
        "kwargs" @ 374..380: Property [readonly]
        "str" @ 385..388: Class
        "str" @ 405..408: Class
        "func" @ 409..413: Parameter
        "args" @ 415..419: Parameter
        "kwargs" @ 423..429: Parameter
        "wrapper" @ 443..450: Function
        "Container" @ 504..513: Class [definition]
        "T" @ 514..515: TypeParameter [definition]
        "U" @ 517..518: TypeParameter [definition]
        "__init__" @ 529..537: Method [definition]
        "self" @ 538..542: SelfParameter [definition]
        "value1" @ 544..550: Parameter [definition]
        "T" @ 552..553: TypeParameter
        "value2" @ 555..561: Parameter [definition]
        "U" @ 563..564: TypeParameter
        "self" @ 575..579: SelfParameter
        "value1" @ 580..586: Variable
        "T" @ 588..589: TypeParameter
        "value1" @ 592..598: Parameter
        "self" @ 607..611: SelfParameter
        "value2" @ 612..618: Variable
        "U" @ 620..621: TypeParameter
        "value2" @ 624..630: Parameter
        "get_first" @ 640..649: Method [definition]
        "self" @ 650..654: SelfParameter [definition]
        "T" @ 659..660: TypeParameter
        "self" @ 677..681: SelfParameter
        "value1" @ 682..688: Variable
        "get_second" @ 698..708: Method [definition]
        "self" @ 709..713: SelfParameter [definition]
        "U" @ 718..719: TypeParameter
        "self" @ 736..740: SelfParameter
        "value2" @ 741..747: Variable
        "BoundedContainer" @ 796..812: Class [definition]
        "T" @ 813..814: TypeParameter [definition]
        "int" @ 816..819: Class
        "U" @ 821..822: TypeParameter [definition]
        "str" @ 825..828: Class
        "process" @ 839..846: Method [definition]
        "self" @ 847..851: SelfParameter [definition]
        "x" @ 853..854: Parameter [definition]
        "T" @ 856..857: TypeParameter
        "y" @ 859..860: Parameter [definition]
        "U" @ 862..863: TypeParameter
        "tuple" @ 868..873: Class
        "T" @ 874..875: TypeParameter
        "U" @ 877..878: TypeParameter
        "x" @ 897..898: Parameter
        "y" @ 900..901: Parameter
        "#);
    }

    #[test]
    fn type_parameters_usage_in_function_body() {
        let test = SemanticTokenTest::new(
            "
def generic_function[T](value: T) -> T:
    # Type parameter T should be recognized here too
    result: T = value
    temp = result  # This could potentially be T as well
    return result
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "generic_function" @ 5..21: Function [definition]
        "T" @ 22..23: TypeParameter [definition]
        "value" @ 25..30: Parameter [definition]
        "T" @ 32..33: TypeParameter
        "T" @ 38..39: TypeParameter
        "result" @ 98..104: Variable [definition]
        "T" @ 106..107: TypeParameter
        "value" @ 110..115: Parameter
        "temp" @ 120..124: Variable [definition]
        "result" @ 127..133: Variable
        "result" @ 184..190: Variable
        "#);
    }

    #[test]
    fn decorator_classification() {
        let test = SemanticTokenTest::new(
            r#"
class App:
    def route(self, path):
        pass

app = App()

@staticmethod
@property
@app.route("/path")
def my_function():
    pass

@dataclass
class MyClass:
    pass
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "App" @ 7..10: Class [definition]
        "route" @ 20..25: Method [definition]
        "self" @ 26..30: SelfParameter [definition]
        "path" @ 32..36: Parameter [definition]
        "app" @ 53..56: Variable [definition]
        "App" @ 59..62: Class
        "staticmethod" @ 67..79: Decorator
        "property" @ 81..89: Decorator
        "app" @ 91..94: Variable
        "route" @ 95..100: Method
        "\"/path\"" @ 101..108: String
        "my_function" @ 114..125: Function [definition]
        "dataclass" @ 140..149: Decorator
        "MyClass" @ 156..163: Class [definition]
        "#);
    }

    #[test]
    fn constant_variations() {
        let test = SemanticTokenTest::new(
            r#"
A = 1
AB = 1
ABC = 1
A1 = 1
AB1 = 1
ABC1 = 1
A_B = 1
A1_B = 1
A_B1 = 1
A_1 = 1
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "A" @ 1..2: Variable [definition]
        "1" @ 5..6: Number
        "AB" @ 7..9: Variable [definition, readonly]
        "1" @ 12..13: Number
        "ABC" @ 14..17: Variable [definition, readonly]
        "1" @ 20..21: Number
        "A1" @ 22..24: Variable [definition, readonly]
        "1" @ 27..28: Number
        "AB1" @ 29..32: Variable [definition, readonly]
        "1" @ 35..36: Number
        "ABC1" @ 37..41: Variable [definition, readonly]
        "1" @ 44..45: Number
        "A_B" @ 46..49: Variable [definition, readonly]
        "1" @ 52..53: Number
        "A1_B" @ 54..58: Variable [definition, readonly]
        "1" @ 61..62: Number
        "A_B1" @ 63..67: Variable [definition, readonly]
        "1" @ 70..71: Number
        "A_1" @ 72..75: Variable [definition, readonly]
        "1" @ 78..79: Number
        "#);
    }

    #[test]
    fn implicitly_concatenated_strings() {
        let test = SemanticTokenTest::new(
            r#"x = "hello" "world"
y = ("multi"
     "line"
     "string")
z = 'single' "mixed" 'quotes'"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 0..1: Variable [definition]
        "\"hello\"" @ 4..11: String
        "\"world\"" @ 12..19: String
        "y" @ 20..21: Variable [definition]
        "\"multi\"" @ 25..32: String
        "\"line\"" @ 38..44: String
        "\"string\"" @ 50..58: String
        "z" @ 60..61: Variable [definition]
        "'single'" @ 64..72: String
        "\"mixed\"" @ 73..80: String
        "'quotes'" @ 81..89: String
        "#);
    }

    #[test]
    fn bytes_literals() {
        let test = SemanticTokenTest::new(
            r#"x = b"hello" b"world"
y = (b"multi"
     b"line"
     b"bytes")
z = b'single' b"mixed" b'quotes'"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 0..1: Variable [definition]
        "b\"hello\"" @ 4..12: String
        "b\"world\"" @ 13..21: String
        "y" @ 22..23: Variable [definition]
        "b\"multi\"" @ 27..35: String
        "b\"line\"" @ 41..48: String
        "b\"bytes\"" @ 54..62: String
        "z" @ 64..65: Variable [definition]
        "b'single'" @ 68..77: String
        "b\"mixed\"" @ 78..86: String
        "b'quotes'" @ 87..96: String
        "#);
    }

    #[test]
    fn mixed_string_and_bytes_literals() {
        let test = SemanticTokenTest::new(
            r#"# Test mixed string and bytes literals
string_concat = "hello" "world"
bytes_concat = b"hello" b"world"
mixed_quotes_str = 'single' "double" 'single'
mixed_quotes_bytes = b'single' b"double" b'single'
regular_string = "just a string"
regular_bytes = b"just bytes""#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "string_concat" @ 39..52: Variable [definition]
        "\"hello\"" @ 55..62: String
        "\"world\"" @ 63..70: String
        "bytes_concat" @ 71..83: Variable [definition]
        "b\"hello\"" @ 86..94: String
        "b\"world\"" @ 95..103: String
        "mixed_quotes_str" @ 104..120: Variable [definition]
        "'single'" @ 123..131: String
        "\"double\"" @ 132..140: String
        "'single'" @ 141..149: String
        "mixed_quotes_bytes" @ 150..168: Variable [definition]
        "b'single'" @ 171..180: String
        "b\"double\"" @ 181..190: String
        "b'single'" @ 191..200: String
        "regular_string" @ 201..215: Variable [definition]
        "\"just a string\"" @ 218..233: String
        "regular_bytes" @ 234..247: Variable [definition]
        "b\"just bytes\"" @ 250..263: String
        "#);
    }

    #[test]
    fn fstring_with_mixed_literals() {
        let test = SemanticTokenTest::new(
            r#"
# Test f-strings with various literal types
name = "Alice"
data = b"hello"
value = 42

# F-string with string literals, expressions, and other literals
result = f"Hello {name}! Value: {value}, Data: {data!r}"

# F-string with concatenated string and bytes literals
mixed = f"prefix" + b"suffix"

# Complex f-string with nested expressions
complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "name" @ 45..49: Variable [definition]
        "\"Alice\"" @ 52..59: String
        "data" @ 60..64: Variable [definition]
        "b\"hello\"" @ 67..75: String
        "value" @ 76..81: Variable [definition]
        "42" @ 84..86: Number
        "result" @ 153..159: Variable [definition]
        "Hello " @ 164..170: String
        "name" @ 171..175: Variable
        "! Value: " @ 176..185: String
        "value" @ 186..191: Variable
        ", Data: " @ 192..200: String
        "data" @ 201..205: Variable
        "mixed" @ 266..271: Variable [definition]
        "prefix" @ 276..282: String
        "b\"suffix\"" @ 286..295: String
        "complex_fstring" @ 340..355: Variable [definition]
        "User: " @ 360..366: String
        "name" @ 367..371: Variable
        "upper" @ 372..377: Method
        ", Count: " @ 380..389: String
        "len" @ 390..393: Function
        "data" @ 394..398: Variable
        ", Hex: " @ 400..407: String
        "value" @ 408..413: Variable
        "x" @ 414..415: String
        "#);
    }

    #[test]
    fn tstring_with_mixed_literals() {
        let test = SemanticTokenTest::new(
            r#"
# Test t-strings with various literal types
name = "Alice"
value = 42

# T-string with string literals and expressions
result = t"Hello {name}! Value: {value}"

# Complex t-string with nested expressions
complex_tstring = t"User: {name.upper()}, Count: {len(name)}"
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "name" @ 45..49: Variable [definition]
        "\"Alice\"" @ 52..59: String
        "value" @ 60..65: Variable [definition]
        "42" @ 68..70: Number
        "result" @ 120..126: Variable [definition]
        "Hello " @ 131..137: String
        "name" @ 138..142: Variable
        "! Value: " @ 143..152: String
        "value" @ 153..158: Variable
        "complex_tstring" @ 205..220: Variable [definition]
        "User: " @ 225..231: String
        "name" @ 232..236: Variable
        "upper" @ 237..242: Method
        ", Count: " @ 245..254: String
        "len" @ 255..258: Function
        "name" @ 259..263: Variable
        "#);
    }

    #[test]
    fn nonlocal_and_global_statements() {
        let test = SemanticTokenTest::new(
            r#"
x = "global_value"
y = "another_global"

def outer():
    x = "outer_value"
    z = "outer_local"

    def inner():
        nonlocal x, z  # These should be variable tokens
        global y       # This should be a variable token
        x = "modified"
        y = "modified_global"
        z = "modified_local"

        def deeper():
            nonlocal x    # Variable token
            global y, x   # Both should be variable tokens
            return x + y

        return deeper

    return inner
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 1..2: Variable [definition]
        "\"global_value\"" @ 5..19: String
        "y" @ 20..21: Variable [definition]
        "\"another_global\"" @ 24..40: String
        "outer" @ 46..51: Function [definition]
        "x" @ 59..60: Variable [definition]
        "\"outer_value\"" @ 63..76: String
        "z" @ 81..82: Variable [definition]
        "\"outer_local\"" @ 85..98: String
        "inner" @ 108..113: Function [definition]
        "x" @ 134..135: Variable
        "z" @ 137..138: Variable
        "y" @ 189..190: Variable
        "x" @ 239..240: Variable [definition]
        "\"modified\"" @ 243..253: String
        "y" @ 262..263: Variable [definition]
        "\"modified_global\"" @ 266..283: String
        "z" @ 292..293: Variable [definition]
        "\"modified_local\"" @ 296..312: String
        "deeper" @ 326..332: Function [definition]
        "x" @ 357..358: Variable
        "y" @ 398..399: Variable
        "x" @ 401..402: Variable
        "x" @ 457..458: Variable
        "y" @ 461..462: Variable
        "deeper" @ 479..485: Function
        "inner" @ 498..503: Function
        "#);
    }

    #[test]
    fn nonlocal_global_edge_cases() {
        let test = SemanticTokenTest::new(
            r#"
# Single variable statements
def test():
    global x
    nonlocal y

    # Multiple variables in one statement
    global a, b, c
    nonlocal d, e, f

    return x + y + a + b + c + d + e + f
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "test" @ 34..38: Function [definition]
        "x" @ 53..54: Variable
        "y" @ 68..69: Variable
        "a" @ 124..125: Variable
        "b" @ 127..128: Variable
        "c" @ 130..131: Variable
        "d" @ 145..146: Variable
        "e" @ 148..149: Variable
        "f" @ 151..152: Variable
        "#);
    }

    #[test]
    fn pattern_matching() {
        let test = SemanticTokenTest::new(
            r#"
def process_data(data):
    match data:
        case {"name": name, "age": age, **rest} as person:
            print(f"Person {name}, age {age}, extra: {rest}")
            return person
        case [first, *remaining] as sequence:
            print(f"First: {first}, remaining: {remaining}")
            return sequence
        case value as fallback:
            print(f"Fallback: {fallback}")
            return fallback
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "process_data" @ 5..17: Function [definition]
        "data" @ 18..22: Parameter [definition]
        "data" @ 35..39: Parameter
        "\"name\"" @ 55..61: String
        "name" @ 63..67: Variable
        "\"age\"" @ 69..74: String
        "age" @ 76..79: Variable
        "rest" @ 83..87: Variable
        "person" @ 92..98: Variable
        "print" @ 112..117: Function
        "Person " @ 120..127: String
        "name" @ 128..132: Variable
        ", age " @ 133..139: String
        "age" @ 140..143: Variable
        ", extra: " @ 144..153: String
        "rest" @ 154..158: Variable
        "person" @ 181..187: Variable
        "first" @ 202..207: Variable
        "remaining" @ 210..219: Variable
        "sequence" @ 224..232: Variable
        "print" @ 246..251: Function
        "First: " @ 254..261: String
        "first" @ 262..267: Variable
        ", remaining: " @ 268..281: String
        "remaining" @ 282..291: Variable
        "sequence" @ 314..322: Variable
        "value" @ 336..341: Variable
        "fallback" @ 345..353: Variable
        "print" @ 367..372: Function
        "Fallback: " @ 375..385: String
        "fallback" @ 386..394: Variable
        "fallback" @ 417..425: Variable
        "#);
    }

    #[test]
    fn exception_handlers() {
        let test = SemanticTokenTest::new(
            r#"
try:
    x = 1 / 0
except ValueError as ve:
    print(ve)
except (TypeError, RuntimeError) as re:
    print(re)
except Exception as e:
    print(e)
finally:
    pass
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 10..11: Variable [definition]
        "1" @ 14..15: Number
        "0" @ 18..19: Number
        "ValueError" @ 27..37: Class
        "ve" @ 41..43: Variable [definition]
        "print" @ 49..54: Function
        "ve" @ 55..57: Variable
        "TypeError" @ 67..76: Class
        "RuntimeError" @ 78..90: Class
        "re" @ 95..97: Variable [definition]
        "print" @ 103..108: Function
        "re" @ 109..111: Variable
        "Exception" @ 120..129: Class
        "e" @ 133..134: Variable [definition]
        "print" @ 140..145: Function
        "e" @ 146..147: Variable
        "#);
    }

    #[test]
    fn self_attribute_expression() {
        let test = SemanticTokenTest::new(
            r#"
from typing import Self


class C:
    def __init__(self: Self):
        self.annotated: int = 1
        self.non_annotated = 1
        self.x.test()
        self.x()
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "typing" @ 6..12: Namespace
        "Self" @ 20..24: Variable
        "C" @ 33..34: Class [definition]
        "__init__" @ 44..52: Method [definition]
        "self" @ 53..57: SelfParameter [definition]
        "Self" @ 59..63: Variable
        "self" @ 74..78: SelfParameter
        "annotated" @ 79..88: Variable
        "int" @ 90..93: Class
        "1" @ 96..97: Number
        "self" @ 106..110: SelfParameter
        "non_annotated" @ 111..124: Variable
        "1" @ 127..128: Number
        "self" @ 137..141: SelfParameter
        "self" @ 159..163: SelfParameter
        "#);
    }

    #[test]
    fn augmented_assignment() {
        let test = SemanticTokenTest::new(
            r#"
x = 0
x += 1
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "x" @ 1..2: Variable [definition]
        "0" @ 5..6: Number
        "x" @ 7..8: Variable
        "1" @ 12..13: Number
        "#);
    }

    #[test]
    fn type_alias() {
        let test = SemanticTokenTest::new("type MyList[T] = list[T]");

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "MyList" @ 5..11: Class [definition]
        "T" @ 12..13: TypeParameter [definition]
        "list" @ 17..21: Class
        "T" @ 22..23: TypeParameter
        "#);
    }

    #[test]
    fn for_stmt() {
        let test = SemanticTokenTest::new(
            r#"
for item in []:
    print(item)
else:
    print(0)
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "item" @ 5..9: Variable [definition]
        "print" @ 21..26: Function
        "item" @ 27..31: Variable
        "print" @ 43..48: Function
        "0" @ 49..50: Number
        "#);
    }

    #[test]
    fn with_stmt() {
        let test = SemanticTokenTest::new(
            r#"
with open("file.txt") as f:
    f.read()
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "open" @ 6..10: Function
        "\"file.txt\"" @ 11..21: String
        "f" @ 26..27: Variable [definition]
        "f" @ 33..34: Variable
        "read" @ 35..39: Method
        "#);
    }

    #[test]
    fn comprehensions() {
        let test = SemanticTokenTest::new(
            r#"
list_comp = [x for x in range(10) if x % 2 == 0]
set_comp = {x for x in range(10)}
dict_comp = {k: v for k, v in zip(["a", "b"], [1, 2])}
generator = (x for x in range(10))
"#,
        );

        let tokens = test.highlight_file();
        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "list_comp" @ 1..10: Variable [definition]
        "x" @ 14..15: Variable
        "x" @ 20..21: Variable [definition]
        "range" @ 25..30: Class
        "10" @ 31..33: Number
        "x" @ 38..39: Variable
        "2" @ 42..43: Number
        "0" @ 47..48: Number
        "set_comp" @ 50..58: Variable [definition]
        "x" @ 62..63: Variable
        "x" @ 68..69: Variable [definition]
        "range" @ 73..78: Class
        "10" @ 79..81: Number
        "dict_comp" @ 84..93: Variable [definition]
        "k" @ 97..98: Variable
        "v" @ 100..101: Variable
        "k" @ 106..107: Variable [definition]
        "v" @ 109..110: Variable [definition]
        "zip" @ 114..117: Class
        "\"a\"" @ 119..122: String
        "\"b\"" @ 124..127: String
        "1" @ 131..132: Number
        "2" @ 134..135: Number
        "generator" @ 139..148: Variable [definition]
        "x" @ 152..153: Variable
        "x" @ 158..159: Variable [definition]
        "range" @ 163..168: Class
        "10" @ 169..171: Number
        "#);
    }

    /// Regression test for <https://github.com/astral-sh/ty/issues/1406>
    #[test]
    fn invalid_kwargs() {
        let test = SemanticTokenTest::new(
            r#"
def foo(self, **key, value=10):
    return
"#,
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "foo" @ 5..8: Function [definition]
        "self" @ 9..13: Parameter [definition]
        "key" @ 17..20: Parameter [definition]
        "value" @ 22..27: Parameter [definition]
        "10" @ 28..30: Number
        "#);
    }

    #[test]
    fn import_as() {
        let test = SemanticTokenTest::new(
            r#"
            import pathlib as path
            from pathlib import Path
            "#,
        );

        let tokens = test.highlight_file();
        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "pathlib" @ 8..15: Namespace
        "path" @ 19..23: Namespace
        "pathlib" @ 29..36: Namespace
        "Path" @ 44..48: Class
        "#);
    }

    #[test]
    fn import_from_as() {
        // Test that both the imported name and its alias get highlighted
        // See: https://github.com/astral-sh/ty/issues/2547
        let test = SemanticTokenTest::new(
            r#"
from pathlib import Path as P
from collections.abc import Set as AbstractSet
"#,
        );

        let tokens = test.highlight_file();
        // Both the imported name and the alias should get the same highlighting
        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "pathlib" @ 6..13: Namespace
        "Path" @ 21..25: Class
        "P" @ 29..30: Class
        "collections" @ 36..47: Namespace
        "abc" @ 48..51: Namespace
        "Set" @ 59..62: Class
        "AbstractSet" @ 66..77: Class
        "#);
    }

    #[test]
    fn unresolved_names_do_not_receive_semantic_tokens() {
        let test = SemanticTokenTest::new(
            r#"
def f():
    missing()
"#,
        );

        let tokens = test.highlight_file();
        assert_snapshot!(test.to_snapshot(&tokens), @r#""f" @ 5..6: Function [definition]"#);
    }

    #[test]
    fn unresolved_attributes_do_not_receive_semantic_tokens() {
        let test = SemanticTokenTest::new(
            r#"
class C: ...

def f(c: C):
    c.missing()
"#,
        );

        let tokens = test.highlight_file();
        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "C" @ 7..8: Class [definition]
        "f" @ 19..20: Function [definition]
        "c" @ 21..22: Parameter [definition]
        "C" @ 24..25: Class
        "c" @ 32..33: Parameter
        "#);
    }

    #[test]
    fn unresolved_imported_names_do_not_receive_semantic_tokens() {
        let test = SemanticTokenTest::new(
            r#"
from pathlib import Missing as Alias
"#,
        );

        let tokens = test.highlight_file();
        assert_snapshot!(test.to_snapshot(&tokens), @r#""pathlib" @ 6..13: Namespace"#);
    }

    pub(super) struct SemanticTokenTest {
        pub(super) db: ty_project::TestDb,
        file: File,
    }

    impl SemanticTokenTest {
        fn new(source: &str) -> Self {
            let mut db = ty_project::TestDb::new(ProjectMetadata::new(
                "test".into(),
                SystemPathBuf::from("/"),
            ));

            db.init_program().unwrap();

            let path = SystemPath::new("src/main.py");
            db.write_file(path, ruff_python_trivia::textwrap::dedent(source))
                .expect("Write to memory file system to always succeed");

            let file = system_path_to_file(&db, path).expect("newly written file to existing");

            Self { db, file }
        }

        /// Get semantic tokens for the entire file
        fn highlight_file(&self) -> SemanticTokens {
            semantic_tokens(&self.db, self.file, None)
        }

        /// Get semantic tokens for a specific range in the file
        fn highlight_range(&self, range: TextRange) -> SemanticTokens {
            semantic_tokens(&self.db, self.file, Some(range))
        }

        /// Helper function to convert semantic tokens to a snapshot-friendly text format
        fn to_snapshot(&self, tokens: &SemanticTokens) -> String {
            use std::fmt::Write;
            let source = ruff_db::source::source_text(&self.db, self.file);
            let mut result = String::new();

            for token in tokens.iter() {
                let token_text = &source[token.range()];
                let modifiers_text = if token.modifiers.is_empty() {
                    String::new()
                } else {
                    let mut mods = Vec::new();
                    if token.modifiers.contains(SemanticTokenModifier::DEFINITION) {
                        mods.push("definition");
                    }
                    if token.modifiers.contains(SemanticTokenModifier::READONLY) {
                        mods.push("readonly");
                    }
                    if token.modifiers.contains(SemanticTokenModifier::ASYNC) {
                        mods.push("async");
                    }
                    if token
                        .modifiers
                        .contains(SemanticTokenModifier::DOCUMENTATION)
                    {
                        mods.push("documentation");
                    }
                    format!(" [{}]", mods.join(", "))
                };

                writeln!(
                    result,
                    "{:?} @ {}..{}: {:?}{}",
                    token_text,
                    u32::from(token.start()),
                    u32::from(token.end()),
                    token.token_type,
                    modifiers_text
                )
                .unwrap();
            }

            result
        }
    }
}
