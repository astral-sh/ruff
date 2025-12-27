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
//! TODO: Need to properly handle Annotated expressions. All type arguments other
//! than the first should be treated as value expressions, not as type expressions.
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
use itertools::Itertools;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{
    SourceOrderVisitor, TraversalSignal, walk_arguments, walk_expr, walk_stmt,
};
use ruff_python_ast::{
    self as ast, AnyNodeRef, BytesLiteral, Expr, FString, InterpolatedStringElement, Stmt,
    StringLiteral, TypeParam,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use std::ops::Deref;
use ty_python_semantic::semantic_index::definition::Definition;
use ty_python_semantic::types::TypeVarKind;
use ty_python_semantic::{
    HasType, SemanticModel, semantic_index::definition::DefinitionKind, types::Type,
    types::ide_support::definition_for_name,
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
    in_type_annotation: bool,
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
            in_type_annotation: false,
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

    fn classify_name(&self, name: &ast::ExprName) -> (SemanticTokenType, SemanticTokenModifier) {
        // First try to classify the token based on its definition kind.
        let definition = definition_for_name(
            self.model,
            name,
            ty_python_semantic::ImportAliasResolution::ResolveAliases,
        );

        if let Some(definition) = definition {
            let name_str = name.id.as_str();
            if let Some(classification) = self.classify_from_definition(definition, name_str) {
                return classification;
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
            DefinitionKind::TypeVar(_) => Some((SemanticTokenType::TypeParameter, modifiers)),
            DefinitionKind::Parameter(parameter) => {
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
            DefinitionKind::VariadicPositionalParameter(_) => {
                Some((SemanticTokenType::Parameter, modifiers))
            }
            DefinitionKind::VariadicKeywordParameter(_) => {
                Some((SemanticTokenType::Parameter, modifiers))
            }
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
    ) -> (SemanticTokenType, SemanticTokenModifier) {
        let mut modifiers = SemanticTokenModifier::empty();

        // In type annotation contexts, names that refer to nominal instances or protocol instances
        // should be classified as Class tokens (e.g., "int" in "x: int" should be a Class token)
        if self.in_type_annotation {
            match ty {
                Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                    return (SemanticTokenType::Class, modifiers);
                }
                _ => {
                    // Continue with normal classification for other types in annotations
                }
            }
        }

        match ty {
            Type::ClassLiteral(_) => (SemanticTokenType::Class, modifiers),
            Type::TypeVar(_) => (SemanticTokenType::TypeParameter, modifiers),
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
        }
    }

    fn classify_from_type_for_attribute(
        ty: Type,
        attr_name: &ast::Identifier,
    ) -> (SemanticTokenType, SemanticTokenModifier) {
        let attr_name_str = attr_name.id.as_str();
        let mut modifiers = SemanticTokenModifier::empty();

        // Classify based on the inferred type of the attribute
        match ty {
            Type::ClassLiteral(_) => (SemanticTokenType::Class, modifiers),
            Type::FunctionLiteral(_) => {
                // This is a function accessed as an attribute, likely a method
                (SemanticTokenType::Method, modifiers)
            }
            Type::BoundMethod(_) => {
                // Method bound to an instance
                (SemanticTokenType::Method, modifiers)
            }
            Type::ModuleLiteral(_) => {
                // Module accessed as an attribute (e.g., from os import path)
                (SemanticTokenType::Namespace, modifiers)
            }
            _ if ty.is_property_instance() => {
                // Actual Python property
                (SemanticTokenType::Property, modifiers)
            }
            _ => {
                // Check for constant naming convention
                if Self::is_constant_name(attr_name_str) {
                    modifiers |= SemanticTokenModifier::READONLY;
                }

                // For other types (variables, constants, etc.), classify as variable
                (SemanticTokenType::Variable, modifiers)
            }
        }
    }

    fn classify_parameter(
        &self,
        _param: &ast::Parameter,
        is_first: bool,
        func: &ast::StmtFunctionDef,
    ) -> SemanticTokenType {
        if is_first && self.in_class_scope {
            // Check if this is a classmethod (has @classmethod decorator)
            // TODO - replace with a more robust way to check whether this is a classmethod
            let is_classmethod =
                func.decorator_list
                    .iter()
                    .any(|decorator| match &decorator.expression {
                        ast::Expr::Name(name) => name.id.as_str() == "classmethod",
                        ast::Expr::Attribute(attr) => attr.attr.id.as_str() == "classmethod",
                        _ => false,
                    });

            // Check if this is a staticmethod (has @staticmethod decorator)
            // TODO - replace with a more robust way to check whether this is a staticmethod
            let is_staticmethod =
                func.decorator_list
                    .iter()
                    .any(|decorator| match &decorator.expression {
                        ast::Expr::Name(name) => name.id.as_str() == "staticmethod",
                        ast::Expr::Attribute(attr) => attr.attr.id.as_str() == "staticmethod",
                        _ => false,
                    });

            if is_staticmethod {
                // Static methods don't have self/cls parameters
                SemanticTokenType::Parameter
            } else if is_classmethod {
                // First parameter of a classmethod is cls parameter
                SemanticTokenType::ClsParameter
            } else {
                // First parameter of an instance method is self parameter
                SemanticTokenType::SelfParameter
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
    ) -> (SemanticTokenType, SemanticTokenModifier) {
        self.classify_from_type_and_name_str(ty, local_name.id.as_str())
    }

    // Visit parameters for a function or lambda expression and classify
    // them as parameters, selfParameter, or clsParameter as appropriate.
    fn visit_parameters(
        &mut self,
        parameters: &ast::Parameters,
        func: Option<&ast::StmtFunctionDef>,
    ) {
        let mut param_index = 0;

        // The `parameters.iter` method does return the parameters in sorted order but only if
        // the AST is well-formed, but e.g. not for:
        // ```py
        // def foo(self, **key, value):
        //     return
        // ```
        // Ideally, the ast would use a single vec for all parameters to avoid this issue as
        // discussed here https://github.com/astral-sh/ruff/issues/14315 and
        // here https://github.com/astral-sh/ruff/blob/71f8389f61a243a0c7584adffc49134ccf792aba/crates/ruff_python_parser/src/parser/statement.rs#L3176-L3179
        let parameters_by_start = parameters
            .iter()
            .sorted_by_key(ruff_text_size::Ranged::start);

        for any_param in parameters_by_start {
            let parameter = any_param.as_parameter();

            let token_type = match any_param {
                ast::AnyParameterRef::NonVariadic(_) => {
                    // For non-variadic parameters (positional-only, regular, keyword-only),
                    // check if this should be classified as self/cls parameter
                    if let Some(func) = func {
                        let result = self.classify_parameter(parameter, param_index == 0, func);
                        param_index += 1;
                        result
                    } else {
                        // For lambdas, all parameters are just parameters (no self/cls)
                        param_index += 1;
                        SemanticTokenType::Parameter
                    }
                }
                ast::AnyParameterRef::Variadic(_) => {
                    // Variadic parameters (*args, **kwargs) are always just parameters
                    param_index += 1;
                    SemanticTokenType::Parameter
                }
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

                self.visit_expr(&type_alias.value);
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
                    if let Some(asname) = &alias.asname {
                        // For aliased imports (from X import Y as Z), classify Z based on what Y is
                        let ty = alias.inferred_type(self.model).unwrap_or(Type::unknown());
                        let (token_type, modifiers) = self.classify_from_alias_type(ty, asname);
                        self.add_token(asname, token_type, modifiers);
                    } else {
                        // For direct imports (from X import Y), use semantic classification
                        let ty = alias.inferred_type(self.model).unwrap_or(Type::unknown());
                        let (token_type, modifiers) =
                            self.classify_from_alias_type(ty, &alias.name);
                        self.add_token(&alias.name, token_type, modifiers);
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

                self.visit_expr(&assignment.annotation);

                if let Some(value) = &assignment.value {
                    self.visit_expr(value);
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

    fn visit_annotation(&mut self, expr: &'_ Expr) {
        let prev_in_type_annotation = self.in_type_annotation;
        self.in_type_annotation = true;
        self.visit_expr(expr);
        self.in_type_annotation = prev_in_type_annotation;
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            ast::Expr::Name(name) => {
                let (token_type, mut modifiers) = self.classify_name(name);
                if self.in_target_creating_definition && name.ctx.is_store() {
                    modifiers |= SemanticTokenModifier::DEFINITION;
                }
                self.add_token(name, token_type, modifiers);
                walk_expr(self, expr);
            }
            ast::Expr::Attribute(attr) => {
                // Visit the base expression first (e.g., 'os' in 'os.path')
                self.visit_expr(&attr.value);

                // Then add token for the attribute name (e.g., 'path' in 'os.path')
                let ty = expr.inferred_type(self.model).unwrap_or(Type::unknown());
                let (token_type, modifiers) =
                    Self::classify_from_type_for_attribute(ty, &attr.attr);
                self.add_token(&attr.attr, token_type, modifiers);
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
                    let mut sub_visitor = SemanticTokenVisitor::new(&sub_model, None);
                    sub_visitor.visit_expr(sub_ast.expr());
                    self.tokens.extend(sub_visitor.tokens);
                } else {
                    walk_expr(self, expr);
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

    fn visit_f_string(&mut self, f_string: &FString) {
        // F-strings contain elements that can be literal strings or expressions
        for element in &f_string.elements {
            match element {
                InterpolatedStringElement::Literal(literal_element) => {
                    // This is a literal string part within the f-string
                    self.add_token(
                        literal_element.range(),
                        SemanticTokenType::String,
                        SemanticTokenModifier::empty(),
                    );
                }
                InterpolatedStringElement::Interpolation(expr_element) => {
                    // This is an expression within the f-string - visit it normally
                    self.visit_expr(&expr_element.expression);

                    // Handle format spec if present
                    if let Some(format_spec) = &expr_element.format_spec {
                        // Format specs can contain their own interpolated elements
                        for spec_element in &format_spec.elements {
                            match spec_element {
                                InterpolatedStringElement::Literal(literal) => {
                                    self.add_token(
                                        literal.range(),
                                        SemanticTokenType::String,
                                        SemanticTokenModifier::empty(),
                                    );
                                }
                                InterpolatedStringElement::Interpolation(nested_expr) => {
                                    self.visit_expr(&nested_expr.expression);
                                }
                            }
                        }
                    }
                }
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
                // Visit keys and patterns in source order by interleaving them
                for (key, nested_pattern) in
                    pattern_mapping.keys.iter().zip(&pattern_mapping.patterns)
                {
                    self.visit_expr(key);
                    self.visit_pattern(nested_pattern);
                }

                // Handle the rest parameter (after "**") - this comes last
                if let Some(rest_name) = &pattern_mapping.rest {
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

        assert_snapshot!(test.to_snapshot(&tokens), @r###"
        "foo" @ 4..7: Function [definition]
        "###);
    }

    #[test]
    fn semantic_tokens_class() {
        let test = SemanticTokenTest::new("class MyClass: pass");

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r###"
        "MyClass" @ 6..13: Class [definition]
        "###);
    }

    #[test]
    fn semantic_tokens_class_args() {
        // This used to cause a panic because of an incorrect
        // insertion-order when visiting arguments inside
        // class definitions.
        let test = SemanticTokenTest::new("class Foo(m=x, m)");

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r###"
        "Foo" @ 6..9: Class [definition]
        "x" @ 12..13: Variable
        "m" @ 15..16: Variable
        "###);
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
        "CONSTANT" @ 140..148: Variable [readonly]
        "my_function" @ 150..161: Variable
        "MyClass" @ 163..170: Variable
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
        "sr" @ 72..74: Variable
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
    fn attribute_classification() {
        let test = SemanticTokenTest::new(
            "
import os
import sys
from collections import defaultdict
from typing import List

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
u = List.__name__        # __name__ should be variable
",
        );

        let tokens = test.highlight_file();

        assert_snapshot!(test.to_snapshot(&tokens), @r#"
        "os" @ 8..10: Namespace
        "sys" @ 18..21: Namespace
        "collections" @ 27..38: Namespace
        "defaultdict" @ 46..57: Class
        "typing" @ 63..69: Namespace
        "List" @ 77..81: Variable
        "MyClass" @ 89..96: Class [definition]
        "CONSTANT" @ 102..110: Variable [definition, readonly]
        "42" @ 113..115: Number
        "method" @ 125..131: Method [definition]
        "self" @ 132..136: SelfParameter [definition]
        "\"hello\"" @ 154..161: String
        "property" @ 168..176: Decorator
        "prop" @ 185..189: Method [definition]
        "self" @ 190..194: SelfParameter [definition]
        "self" @ 212..216: SelfParameter
        "CONSTANT" @ 217..225: Variable [readonly]
        "obj" @ 227..230: Variable [definition]
        "MyClass" @ 233..240: Class
        "x" @ 278..279: Variable [definition]
        "os" @ 282..284: Namespace
        "path" @ 285..289: Namespace
        "y" @ 339..340: Variable [definition]
        "obj" @ 343..346: Variable
        "method" @ 347..353: Method
        "z" @ 405..406: Variable [definition]
        "obj" @ 409..412: Variable
        "CONSTANT" @ 413..421: Variable [readonly]
        "w" @ 483..484: Variable [definition]
        "obj" @ 487..490: Variable
        "prop" @ 491..495: Variable
        "v" @ 534..535: Variable [definition]
        "MyClass" @ 538..545: Class
        "method" @ 546..552: Method
        "u" @ 596..597: Variable [definition]
        "List" @ 600..604: Variable
        "__name__" @ 605..613: Variable
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
        "unknown_attr" @ 195..207: Variable
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
        "Callable" @ 292..300: Variable
        "P" @ 301..302: Variable
        "int" @ 304..307: Class
        "Callable" @ 313..321: Variable
        "P" @ 322..323: Variable
        "str" @ 325..328: Class
        "wrapper" @ 339..346: Function [definition]
        "args" @ 348..352: Parameter [definition]
        "P" @ 354..355: Variable
        "args" @ 356..360: Variable
        "kwargs" @ 364..370: Parameter [definition]
        "P" @ 372..373: Variable
        "kwargs" @ 374..380: Variable
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

        assert_snapshot!(test.to_snapshot(&tokens), @r###"
        "staticmethod" @ 2..14: Decorator
        "property" @ 16..24: Decorator
        "app" @ 26..29: Variable
        "route" @ 30..35: Variable
        "\"/path\"" @ 36..43: String
        "my_function" @ 49..60: Function [definition]
        "dataclass" @ 75..84: Decorator
        "MyClass" @ 91..98: Class [definition]
        "###);
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
        "x" @ 165..166: Variable
        "y" @ 169..170: Variable
        "a" @ 173..174: Variable
        "b" @ 177..178: Variable
        "c" @ 181..182: Variable
        "d" @ 185..186: Variable
        "e" @ 189..190: Variable
        "f" @ 193..194: Variable
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
        "x" @ 142..143: Variable
        "test" @ 144..148: Variable
        "self" @ 159..163: SelfParameter
        "x" @ 164..165: Variable
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
