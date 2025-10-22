use crate::Db;
use bitflags::bitflags;
use itertools::Itertools;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::visitor::source_order::{
    SourceOrderVisitor, TraversalSignal, walk_expr, walk_stmt,
};
use ruff_python_ast::{
    AnyNodeRef, BytesLiteral, Expr, FString, InterpolatedStringElement, Stmt, StringLiteral,
    TypeParam,
};
use ruff_text_size::{Ranged, TextLen, TextRange};
use std::ops::Deref;
use ty_python_semantic::{
    HasType, SemanticModel, semantic_index::definition::DefinitionKind, types::Type,
    types::ide_support::definition_kind_for_name,
};

// This module walks the AST and collects a set of "semantic tokens" for a file
// or a range within a file. Each semantic token provides a "token type" and zero
// or more "modifiers". This information can be used by an editor to provide
// color coding based on semantic meaning.

// Current limitations and areas for future improvement:

// TODO: Need to provide better classification for name tokens that are imported
// from other modules. Currently, these are classified based on their types,
// which often means they're classified as variables when they should be classes
// in many cases.

// TODO: Need to handle semantic tokens within quoted annotations.

// TODO: Need to properly handle Annotated expressions. All type arguments other
// than the first should be treated as value expressions, not as type expressions.

// TODO: An identifier that resolves to a parameter when used within a function
// should be classified as a parameter, selfParameter, or clsParameter token.

// TODO: Properties (or perhaps more generally, descriptor objects?) should be
// classified as property tokens rather than just variables.

// TODO: Special forms like Protocol and TypedDict should probably be classified
// as class tokens, but they are currently classified as variables.

// TODO: Type aliases (including those defined with the Python 3.12 "type" statement)
// do not currently have a dedicated semantic token type, but they maybe should.

// TODO: Additional token modifiers might be added (e.g. for static methods,
// abstract methods and classes).

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
        vec!["definition", "readonly", "async"]
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
    let semantic_model = SemanticModel::new(db, file);

    let mut visitor = SemanticTokenVisitor::new(&semantic_model, file, range);
    visitor.visit_body(parsed.suite());

    SemanticTokens::new(visitor.tokens)
}

/// AST visitor that collects semantic tokens.
struct SemanticTokenVisitor<'db> {
    semantic_model: &'db SemanticModel<'db>,
    file: File,
    tokens: Vec<SemanticToken>,
    in_class_scope: bool,
    in_type_annotation: bool,
    range_filter: Option<TextRange>,
}

impl<'db> SemanticTokenVisitor<'db> {
    fn new(
        semantic_model: &'db SemanticModel<'db>,
        file: File,
        range_filter: Option<TextRange>,
    ) -> Self {
        Self {
            semantic_model,
            file,
            tokens: Vec::new(),
            in_class_scope: false,
            in_type_annotation: false,
            range_filter,
        }
    }

    fn add_token(
        &mut self,
        ranged: impl Ranged,
        token_type: SemanticTokenType,
        modifiers: SemanticTokenModifier,
    ) {
        let range = ranged.range();
        // Only emit tokens that intersect with the range filter, if one is specified
        if let Some(range_filter) = self.range_filter {
            if range.intersect(range_filter).is_none() {
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
        name.chars().all(|c| c.is_uppercase() || c == '_') && name.len() > 1
    }

    fn classify_name(&self, name: &ast::ExprName) -> (SemanticTokenType, SemanticTokenModifier) {
        // First try to classify the token based on its definition kind.
        let definition_kind = definition_kind_for_name(self.semantic_model.db(), self.file, name);

        if let Some(definition_kind) = definition_kind {
            let name_str = name.id.as_str();
            if let Some(classification) =
                self.classify_from_definition_kind(&definition_kind, name_str)
            {
                return classification;
            }
        }

        // Fall back to type-based classification.
        let ty = name.inferred_type(self.semantic_model);
        let name_str = name.id.as_str();
        self.classify_from_type_and_name_str(ty, name_str)
    }

    fn classify_from_definition_kind(
        &self,
        definition_kind: &DefinitionKind<'_>,
        name_str: &str,
    ) -> Option<(SemanticTokenType, SemanticTokenModifier)> {
        let mut modifiers = SemanticTokenModifier::empty();

        match definition_kind {
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
            DefinitionKind::Parameter(_) => Some((SemanticTokenType::Parameter, modifiers)),
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
        let mut current_offset = ruff_text_size::TextSize::default();
        for part in name_str.split('.') {
            if !part.is_empty() {
                self.add_token(
                    ruff_text_size::TextRange::at(name_start + current_offset, part.text_len()),
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
                SemanticTokenModifier::empty(),
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
                self.visit_body(&func.body);
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
                    // Visit base class arguments
                    for arg in &arguments.args {
                        self.visit_expr(arg);
                    }
                    // Visit keyword arguments (for metaclass, etc.)
                    for keyword in &arguments.keywords {
                        self.visit_expr(&keyword.value);
                    }
                }

                let prev_in_class = self.in_class_scope;
                self.in_class_scope = true;
                self.visit_body(&class.body);
                self.in_class_scope = prev_in_class;
            }
            ast::Stmt::Import(import) => {
                for alias in &import.names {
                    if let Some(asname) = &alias.asname {
                        self.add_token(
                            asname.range(),
                            SemanticTokenType::Namespace,
                            SemanticTokenModifier::empty(),
                        );
                    } else {
                        // Create separate tokens for each part of a dotted module name
                        self.add_dotted_name_tokens(&alias.name, SemanticTokenType::Namespace);
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
                        let ty = alias.inferred_type(self.semantic_model);
                        let (token_type, modifiers) = self.classify_from_alias_type(ty, asname);
                        self.add_token(asname, token_type, modifiers);
                    } else {
                        // For direct imports (from X import Y), use semantic classification
                        let ty = alias.inferred_type(self.semantic_model);
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
                let (token_type, modifiers) = self.classify_name(name);
                self.add_token(name, token_type, modifiers);
                walk_expr(self, expr);
            }
            ast::Expr::Attribute(attr) => {
                // Visit the base expression first (e.g., 'os' in 'os.path')
                self.visit_expr(&attr.value);

                // Then add token for the attribute name (e.g., 'path' in 'os.path')
                let ty = expr.inferred_type(self.semantic_model);
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
            _ => {
                // For all other expression types, let the default visitor handle them
                walk_expr(self, expr);
            }
        }
    }

    fn visit_string_literal(&mut self, string_literal: &StringLiteral) {
        // Emit a semantic token for this string literal part
        self.add_token(
            string_literal.range(),
            SemanticTokenType::String,
            SemanticTokenModifier::empty(),
        );
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
            _ => {
                // For all other pattern types, use the default walker
                ruff_python_ast::visitor::source_order::walk_pattern(self, pattern);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::cursor_test;
    use insta::assert_snapshot;

    /// Helper function to get semantic tokens for full file (for testing)
    fn semantic_tokens_full_file(db: &dyn Db, file: File) -> SemanticTokens {
        semantic_tokens(db, file, None)
    }

    /// Helper function to convert semantic tokens to a snapshot-friendly text format
    fn semantic_tokens_to_snapshot(db: &dyn Db, file: File, tokens: &SemanticTokens) -> String {
        use std::fmt::Write;
        let source = ruff_db::source::source_text(db, file);
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
                format!(" [{}]", mods.join(", "))
            };

            writeln!(
                result,
                "{:?} @ {}..{}: {:?}{}",
                token_text,
                u32::from(token.range().start()),
                u32::from(token.range().end()),
                token.token_type,
                modifiers_text
            )
            .unwrap();
        }

        result
    }

    #[test]
    fn test_semantic_tokens_basic() {
        let test = cursor_test("def foo(): pass<CURSOR>");

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r###"
        "foo" @ 4..7: Function [definition]
        "###);
    }

    #[test]
    fn test_semantic_tokens_class() {
        let test = cursor_test("class MyClass: pass<CURSOR>");

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r###"
        "MyClass" @ 6..13: Class [definition]
        "###);
    }

    #[test]
    fn test_semantic_tokens_variables() {
        let test = cursor_test(
            "
x = 42
y = 'hello'<CURSOR>
",
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r###"
        "x" @ 1..2: Variable
        "42" @ 5..7: Number
        "y" @ 8..9: Variable
        "'hello'" @ 12..19: String
        "###);
    }

    #[test]
    fn test_semantic_tokens_self_parameter() {
        let test = cursor_test(
            "
class MyClass:
    def method(self, x): pass<CURSOR>
",
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r###"
        "MyClass" @ 7..14: Class [definition]
        "method" @ 24..30: Method [definition]
        "self" @ 31..35: SelfParameter
        "x" @ 37..38: Parameter
        "###);
    }

    #[test]
    fn test_semantic_tokens_cls_parameter() {
        let test = cursor_test(
            "
class MyClass:
    @classmethod
    def method(cls, x): pass<CURSOR>
",
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "classmethod" @ 21..32: Decorator
        "method" @ 41..47: Method [definition]
        "cls" @ 48..51: ClsParameter
        "x" @ 53..54: Parameter
        "#);
    }

    #[test]
    fn test_semantic_tokens_staticmethod_parameter() {
        let test = cursor_test(
            "
class MyClass:
    @staticmethod
    def method(x, y): pass<CURSOR>
",
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "staticmethod" @ 21..33: Decorator
        "method" @ 42..48: Method [definition]
        "x" @ 49..50: Parameter
        "y" @ 52..53: Parameter
        "#);
    }

    #[test]
    fn test_semantic_tokens_custom_self_cls_names() {
        let test = cursor_test(
            "
class MyClass:
    def method(instance, x): pass
    @classmethod
    def other(klass, y): pass
    def complex_method(instance, posonly, /, regular, *args, kwonly, **kwargs): pass<CURSOR>
",
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "method" @ 24..30: Method [definition]
        "instance" @ 31..39: SelfParameter
        "x" @ 41..42: Parameter
        "classmethod" @ 55..66: Decorator
        "other" @ 75..80: Method [definition]
        "klass" @ 81..86: ClsParameter
        "y" @ 88..89: Parameter
        "complex_method" @ 105..119: Method [definition]
        "instance" @ 120..128: SelfParameter
        "posonly" @ 130..137: Parameter
        "regular" @ 142..149: Parameter
        "args" @ 152..156: Parameter
        "kwonly" @ 158..164: Parameter
        "kwargs" @ 168..174: Parameter
        "#);
    }

    #[test]
    fn test_semantic_tokens_modifiers() {
        let test = cursor_test(
            "
class MyClass:
    CONSTANT = 42
    async def method(self): pass<CURSOR>
",
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r###"
        "MyClass" @ 7..14: Class [definition]
        "CONSTANT" @ 20..28: Variable [readonly]
        "42" @ 31..33: Number
        "method" @ 48..54: Method [definition, async]
        "self" @ 55..59: SelfParameter
        "###);
    }

    #[test]
    fn test_semantic_classification_vs_heuristic() {
        let test = cursor_test(
            "
import sys
class MyClass:
    pass

def my_function():
    return 42

x = MyClass()
y = my_function()
z = sys.version<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "sys" @ 8..11: Namespace
        "MyClass" @ 18..25: Class [definition]
        "my_function" @ 41..52: Function [definition]
        "42" @ 67..69: Number
        "x" @ 71..72: Variable
        "MyClass" @ 75..82: Class
        "y" @ 85..86: Variable
        "my_function" @ 89..100: Function
        "z" @ 103..104: Variable
        "sys" @ 107..110: Namespace
        "version" @ 111..118: Variable
        "#);
    }

    #[test]
    fn test_builtin_constants() {
        let test = cursor_test(
            "
x = True
y = False
z = None<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r###"
        "x" @ 1..2: Variable
        "True" @ 5..9: BuiltinConstant
        "y" @ 10..11: Variable
        "False" @ 14..19: BuiltinConstant
        "z" @ 20..21: Variable
        "None" @ 24..28: BuiltinConstant
        "###);
    }

    #[test]
    fn test_builtin_constants_in_expressions() {
        let test = cursor_test(
            "
def check(value):
    if value is None:
        return False
    return True

result = check(None)<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "check" @ 5..10: Function [definition]
        "value" @ 11..16: Parameter
        "value" @ 26..31: Variable
        "None" @ 35..39: BuiltinConstant
        "False" @ 56..61: BuiltinConstant
        "True" @ 73..77: BuiltinConstant
        "result" @ 79..85: Variable
        "check" @ 88..93: Function
        "None" @ 94..98: BuiltinConstant
        "#);
    }

    #[test]
    fn test_semantic_tokens_range() {
        let test = cursor_test(
            "
def function1():
    x = 42
    return x

def function2():
    y = \"hello\"
    z = True
    return y + z<CURSOR>
",
        );

        let full_tokens = semantic_tokens(&test.db, test.cursor.file, None);

        // Get the range that covers only the second function
        // Hardcoded offsets: function2 starts at position 42, source ends at position 108
        let range = ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::from(42u32),
            ruff_text_size::TextSize::from(108u32),
        );

        let range_tokens = semantic_tokens(&test.db, test.cursor.file, Some(range));

        // Range-based tokens should have fewer tokens than full scan
        // (should exclude tokens from function1)
        assert!(range_tokens.len() < full_tokens.len());

        // Test both full tokens and range tokens with snapshots
        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &full_tokens), @r#"
        "function1" @ 5..14: Function [definition]
        "x" @ 22..23: Variable
        "42" @ 26..28: Number
        "x" @ 40..41: Variable
        "function2" @ 47..56: Function [definition]
        "y" @ 64..65: Variable
        "/"hello/"" @ 68..75: String
        "z" @ 80..81: Variable
        "True" @ 84..88: BuiltinConstant
        "y" @ 100..101: Variable
        "z" @ 104..105: Variable
        "#);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &range_tokens), @r#"
        "function2" @ 47..56: Function [definition]
        "y" @ 64..65: Variable
        "/"hello/"" @ 68..75: String
        "z" @ 80..81: Variable
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

    #[test]
    fn test_dotted_module_names() {
        let test = cursor_test(
            "
import os.path
import sys.version_info
from urllib.parse import urlparse
from collections.abc import Mapping<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
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
    fn test_module_type_classification() {
        let test = cursor_test(
            "
import os
import sys
from collections import defaultdict

# os and sys should be classified as namespace/module types
x = os
y = sys<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "os" @ 8..10: Namespace
        "sys" @ 18..21: Namespace
        "collections" @ 27..38: Namespace
        "defaultdict" @ 46..57: Class
        "x" @ 119..120: Namespace
        "os" @ 123..125: Namespace
        "y" @ 126..127: Namespace
        "sys" @ 130..133: Namespace
        "#);
    }

    #[test]
    fn test_import_classification() {
        let test = cursor_test(
            "
from os import path
from collections import defaultdict, OrderedDict, Counter
from typing import List, Dict, Optional
from mymodule import CONSTANT, my_function, MyClass<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
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
    fn test_attribute_classification() {
        let test = cursor_test(
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
u = List.__name__        # __name__ should be variable<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "os" @ 8..10: Namespace
        "sys" @ 18..21: Namespace
        "collections" @ 27..38: Namespace
        "defaultdict" @ 46..57: Class
        "typing" @ 63..69: Namespace
        "List" @ 77..81: Variable
        "MyClass" @ 89..96: Class [definition]
        "CONSTANT" @ 102..110: Variable [readonly]
        "42" @ 113..115: Number
        "method" @ 125..131: Method [definition]
        "self" @ 132..136: SelfParameter
        "/"hello/"" @ 154..161: String
        "property" @ 168..176: Decorator
        "prop" @ 185..189: Method [definition]
        "self" @ 190..194: SelfParameter
        "self" @ 212..216: Variable
        "CONSTANT" @ 217..225: Variable [readonly]
        "obj" @ 227..230: Variable
        "MyClass" @ 233..240: Class
        "x" @ 278..279: Namespace
        "os" @ 282..284: Namespace
        "path" @ 285..289: Namespace
        "y" @ 339..340: Method
        "obj" @ 343..346: Variable
        "method" @ 347..353: Method
        "z" @ 405..406: Variable
        "obj" @ 409..412: Variable
        "CONSTANT" @ 413..421: Variable [readonly]
        "w" @ 483..484: Variable
        "obj" @ 487..490: Variable
        "prop" @ 491..495: Variable
        "v" @ 534..535: Function
        "MyClass" @ 538..545: Class
        "method" @ 546..552: Method
        "u" @ 596..597: Variable
        "List" @ 600..604: Variable
        "__name__" @ 605..613: Variable
        "#);
    }

    #[test]
    fn test_attribute_fallback_classification() {
        let test = cursor_test(
            "
class MyClass:
    some_attr = \"value\"
    
obj = MyClass()
# Test attribute that might not have detailed semantic info
x = obj.some_attr        # Should fall back to variable, not property
y = obj.unknown_attr     # Should fall back to variable<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "some_attr" @ 20..29: Variable
        "/"value/"" @ 32..39: String
        "obj" @ 41..44: Variable
        "MyClass" @ 47..54: Class
        "x" @ 117..118: Variable
        "obj" @ 121..124: Variable
        "some_attr" @ 125..134: Variable
        "y" @ 187..188: Variable
        "obj" @ 191..194: Variable
        "unknown_attr" @ 195..207: Variable
        "#);
    }

    #[test]
    fn test_constant_name_detection() {
        let test = cursor_test(
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
w = obj.A             # Should not have readonly modifier (length == 1)<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "UPPER_CASE" @ 20..30: Variable [readonly]
        "42" @ 33..35: Number
        "lower_case" @ 40..50: Variable
        "24" @ 53..55: Number
        "MixedCase" @ 60..69: Variable
        "12" @ 72..74: Number
        "A" @ 79..80: Variable
        "1" @ 83..84: Number
        "obj" @ 86..89: Variable
        "MyClass" @ 92..99: Class
        "x" @ 102..103: Variable
        "obj" @ 106..109: Variable
        "UPPER_CASE" @ 110..120: Variable [readonly]
        "y" @ 156..157: Variable
        "obj" @ 160..163: Variable
        "lower_case" @ 164..174: Variable
        "z" @ 216..217: Variable
        "obj" @ 220..223: Variable
        "MixedCase" @ 224..233: Variable
        "w" @ 274..275: Variable
        "obj" @ 278..281: Variable
        "A" @ 282..283: Variable
        "#);
    }

    #[test]
    fn test_type_annotations() {
        let test = cursor_test(
            r#"
from typing import List, Optional

def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
    pass

x: int = 42
y: Optional[str] = None<CURSOR>
"#,
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "typing" @ 6..12: Namespace
        "List" @ 20..24: Variable
        "Optional" @ 26..34: Variable
        "function_with_annotations" @ 40..65: Function [definition]
        "param1" @ 66..72: Parameter
        "int" @ 74..77: Class
        "param2" @ 79..85: Parameter
        "str" @ 87..90: Class
        "Optional" @ 95..103: Variable
        "List" @ 104..108: Variable
        "str" @ 109..112: Class
        "x" @ 126..127: Variable
        "int" @ 129..132: Class
        "42" @ 135..137: Number
        "y" @ 138..139: Variable
        "Optional" @ 141..149: Variable
        "str" @ 150..153: Class
        "None" @ 157..161: BuiltinConstant
        "#);
    }

    #[test]
    fn test_debug_int_classification() {
        let test = cursor_test(
            "
x: int = 42<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r###"
        "x" @ 1..2: Variable
        "int" @ 4..7: Class
        "42" @ 10..12: Number
        "###);
    }

    #[test]
    fn test_debug_user_defined_type_classification() {
        let test = cursor_test(
            "
class MyClass:
    pass

x: MyClass = MyClass()<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "MyClass" @ 7..14: Class [definition]
        "x" @ 26..27: Variable
        "MyClass" @ 29..36: Class
        "MyClass" @ 39..46: Class
        "#);
    }

    #[test]
    fn test_type_annotation_vs_variable_classification() {
        let test = cursor_test(
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
    return None<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "typing" @ 6..12: Namespace
        "List" @ 20..24: Variable
        "Optional" @ 26..34: Variable
        "MyClass" @ 42..49: Class [definition]
        "test_function" @ 65..78: Function [definition]
        "param" @ 79..84: Parameter
        "int" @ 86..89: Class
        "other" @ 91..96: Parameter
        "MyClass" @ 98..105: Class
        "Optional" @ 110..118: Variable
        "List" @ 119..123: Variable
        "str" @ 124..127: Class
        "x" @ 190..191: Variable
        "int" @ 193..196: Class
        "42" @ 199..201: Number
        "y" @ 206..207: Variable
        "MyClass" @ 209..216: Class
        "MyClass" @ 219..226: Class
        "z" @ 233..234: Variable
        "List" @ 236..240: Variable
        "str" @ 241..244: Class
        "/"hello/"" @ 249..256: String
        "None" @ 357..361: BuiltinConstant
        "#);
    }

    #[test]
    fn test_protocol_types_in_annotations() {
        let test = cursor_test(
            "
from typing import Protocol

class MyProtocol(Protocol):
    def method(self) -> int: ...

def test_function(param: MyProtocol) -> None:
    pass
<CURSOR>",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "typing" @ 6..12: Namespace
        "Protocol" @ 20..28: Variable
        "MyProtocol" @ 36..46: Class [definition]
        "Protocol" @ 47..55: Variable
        "method" @ 66..72: Method [definition]
        "self" @ 73..77: SelfParameter
        "int" @ 82..85: Class
        "test_function" @ 96..109: Function [definition]
        "param" @ 110..115: Parameter
        "MyProtocol" @ 117..127: Class
        "None" @ 132..136: BuiltinConstant
        "#);
    }

    #[test]
    fn test_protocol_type_annotation_vs_value_context() {
        let test = cursor_test(
            "
from typing import Protocol

class MyProtocol(Protocol):
    def method(self) -> int: ...

# Value context - MyProtocol is still a class literal, so should be Class
my_protocol_var = MyProtocol

# Type annotation context - should be Class  
def test_function(param: MyProtocol) -> MyProtocol:
    return param
<CURSOR>",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "typing" @ 6..12: Namespace
        "Protocol" @ 20..28: Variable
        "MyProtocol" @ 36..46: Class [definition]
        "Protocol" @ 47..55: Variable
        "method" @ 66..72: Method [definition]
        "self" @ 73..77: SelfParameter
        "int" @ 82..85: Class
        "my_protocol_var" @ 166..181: Class
        "MyProtocol" @ 184..194: Class
        "test_function" @ 246..259: Function [definition]
        "param" @ 260..265: Parameter
        "MyProtocol" @ 267..277: Class
        "MyProtocol" @ 282..292: Class
        "param" @ 305..310: Parameter
        "#);
    }

    #[test]
    fn test_type_parameters_pep695() {
        let test = cursor_test(
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
<CURSOR>",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "func" @ 87..91: Function [definition]
        "T" @ 92..93: TypeParameter [definition]
        "x" @ 95..96: Parameter
        "T" @ 98..99: TypeParameter
        "T" @ 104..105: TypeParameter
        "x" @ 118..119: Parameter
        "func_tuple" @ 164..174: Function [definition]
        "Ts" @ 176..178: TypeParameter [definition]
        "args" @ 180..184: Parameter
        "tuple" @ 186..191: Class
        "Ts" @ 193..195: Variable
        "tuple" @ 201..206: Class
        "Ts" @ 208..210: Variable
        "args" @ 224..228: Parameter
        "func_paramspec" @ 268..282: Function [definition]
        "P" @ 285..286: TypeParameter [definition]
        "func" @ 288..292: Parameter
        "Callable" @ 294..302: Variable
        "P" @ 303..304: Variable
        "int" @ 306..309: Class
        "Callable" @ 315..323: Variable
        "P" @ 324..325: Variable
        "str" @ 327..330: Class
        "wrapper" @ 341..348: Function [definition]
        "args" @ 350..354: Parameter
        "P" @ 356..357: Variable
        "args" @ 358..362: Variable
        "kwargs" @ 366..372: Parameter
        "P" @ 374..375: Variable
        "kwargs" @ 376..382: Variable
        "str" @ 387..390: Class
        "str" @ 407..410: Class
        "func" @ 411..415: Variable
        "args" @ 417..421: Parameter
        "kwargs" @ 425..431: Parameter
        "wrapper" @ 445..452: Function
        "Container" @ 506..515: Class [definition]
        "T" @ 516..517: TypeParameter [definition]
        "U" @ 519..520: TypeParameter [definition]
        "__init__" @ 531..539: Method [definition]
        "self" @ 540..544: SelfParameter
        "value1" @ 546..552: Parameter
        "T" @ 554..555: TypeParameter
        "value2" @ 557..563: Parameter
        "U" @ 565..566: TypeParameter
        "self" @ 577..581: TypeParameter
        "value1" @ 582..588: Variable
        "T" @ 590..591: TypeParameter
        "value1" @ 594..600: Parameter
        "self" @ 609..613: TypeParameter
        "value2" @ 614..620: Variable
        "U" @ 622..623: TypeParameter
        "value2" @ 626..632: Parameter
        "get_first" @ 642..651: Method [definition]
        "self" @ 652..656: SelfParameter
        "T" @ 661..662: TypeParameter
        "self" @ 679..683: TypeParameter
        "value1" @ 684..690: Variable
        "get_second" @ 700..710: Method [definition]
        "self" @ 711..715: SelfParameter
        "U" @ 720..721: TypeParameter
        "self" @ 738..742: TypeParameter
        "value2" @ 743..749: Variable
        "BoundedContainer" @ 798..814: Class [definition]
        "T" @ 815..816: TypeParameter [definition]
        "int" @ 818..821: Class
        "U" @ 823..824: TypeParameter [definition]
        "str" @ 827..830: Class
        "process" @ 841..848: Method [definition]
        "self" @ 849..853: SelfParameter
        "x" @ 855..856: Parameter
        "T" @ 858..859: TypeParameter
        "y" @ 861..862: Parameter
        "U" @ 864..865: TypeParameter
        "tuple" @ 870..875: Class
        "T" @ 876..877: TypeParameter
        "U" @ 879..880: TypeParameter
        "x" @ 899..900: Parameter
        "y" @ 902..903: Parameter
        "#);
    }

    #[test]
    fn test_type_parameters_usage_in_function_body() {
        let test = cursor_test(
            "
def generic_function[T](value: T) -> T:
    # Type parameter T should be recognized here too
    result: T = value
    temp = result  # This could potentially be T as well
    return result
<CURSOR>",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "generic_function" @ 5..21: Function [definition]
        "T" @ 22..23: TypeParameter [definition]
        "value" @ 25..30: Parameter
        "T" @ 32..33: TypeParameter
        "T" @ 38..39: TypeParameter
        "result" @ 98..104: Variable
        "T" @ 106..107: TypeParameter
        "value" @ 110..115: Parameter
        "temp" @ 120..124: TypeParameter
        "result" @ 127..133: Variable
        "result" @ 184..190: Variable
        "#);
    }

    #[test]
    fn test_decorator_classification() {
        let test = cursor_test(
            r#"
@staticmethod
@property
@app.route("/path")
def my_function():
    pass

@dataclass
class MyClass:
    pass<CURSOR>
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "staticmethod" @ 2..14: Decorator
        "property" @ 16..24: Decorator
        "app" @ 26..29: Variable
        "route" @ 30..35: Variable
        "/"/path/"" @ 36..43: String
        "my_function" @ 49..60: Function [definition]
        "dataclass" @ 75..84: Decorator
        "MyClass" @ 91..98: Class [definition]
        "#);
    }

    #[test]
    fn test_implicitly_concatenated_strings() {
        let test = cursor_test(
            r#"x = "hello" "world"
y = ("multi" 
     "line" 
     "string")
z = 'single' "mixed" 'quotes'<CURSOR>"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "x" @ 0..1: Variable
        "/"hello/"" @ 4..11: String
        "/"world/"" @ 12..19: String
        "y" @ 20..21: Variable
        "/"multi/"" @ 25..32: String
        "/"line/"" @ 39..45: String
        "/"string/"" @ 52..60: String
        "z" @ 62..63: Variable
        "'single'" @ 66..74: String
        "/"mixed/"" @ 75..82: String
        "'quotes'" @ 83..91: String
        "#);
    }

    #[test]
    fn test_bytes_literals() {
        let test = cursor_test(
            r#"x = b"hello" b"world"
y = (b"multi" 
     b"line" 
     b"bytes")
z = b'single' b"mixed" b'quotes'<CURSOR>"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "x" @ 0..1: Variable
        "b/"hello/"" @ 4..12: String
        "b/"world/"" @ 13..21: String
        "y" @ 22..23: Variable
        "b/"multi/"" @ 27..35: String
        "b/"line/"" @ 42..49: String
        "b/"bytes/"" @ 56..64: String
        "z" @ 66..67: Variable
        "b'single'" @ 70..79: String
        "b/"mixed/"" @ 80..88: String
        "b'quotes'" @ 89..98: String
        "#);
    }

    #[test]
    fn test_mixed_string_and_bytes_literals() {
        let test = cursor_test(
            r#"# Test mixed string and bytes literals
string_concat = "hello" "world"
bytes_concat = b"hello" b"world"
mixed_quotes_str = 'single' "double" 'single'
mixed_quotes_bytes = b'single' b"double" b'single'
regular_string = "just a string"
regular_bytes = b"just bytes"<CURSOR>"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "string_concat" @ 39..52: Variable
        "/"hello/"" @ 55..62: String
        "/"world/"" @ 63..70: String
        "bytes_concat" @ 71..83: Variable
        "b/"hello/"" @ 86..94: String
        "b/"world/"" @ 95..103: String
        "mixed_quotes_str" @ 104..120: Variable
        "'single'" @ 123..131: String
        "/"double/"" @ 132..140: String
        "'single'" @ 141..149: String
        "mixed_quotes_bytes" @ 150..168: Variable
        "b'single'" @ 171..180: String
        "b/"double/"" @ 181..190: String
        "b'single'" @ 191..200: String
        "regular_string" @ 201..215: Variable
        "/"just a string/"" @ 218..233: String
        "regular_bytes" @ 234..247: Variable
        "b/"just bytes/"" @ 250..263: String
        "#);
    }

    #[test]
    fn test_fstring_with_mixed_literals() {
        let test = cursor_test(
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
complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"<CURSOR>
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "name" @ 45..49: Variable
        "/"Alice/"" @ 52..59: String
        "data" @ 60..64: Variable
        "b/"hello/"" @ 67..75: String
        "value" @ 76..81: Variable
        "42" @ 84..86: Number
        "result" @ 153..159: Variable
        "Hello " @ 164..170: String
        "name" @ 171..175: Variable
        "! Value: " @ 176..185: String
        "value" @ 186..191: Variable
        ", Data: " @ 192..200: String
        "data" @ 201..205: Variable
        "mixed" @ 266..271: Variable
        "prefix" @ 276..282: String
        "b/"suffix/"" @ 286..295: String
        "complex_fstring" @ 340..355: Variable
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
    fn test_nonlocal_and_global_statements() {
        let test = cursor_test(
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
    
    return inner<CURSOR>
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "x" @ 1..2: Variable
        "/"global_value/"" @ 5..19: String
        "y" @ 20..21: Variable
        "/"another_global/"" @ 24..40: String
        "outer" @ 46..51: Function [definition]
        "x" @ 59..60: Variable
        "/"outer_value/"" @ 63..76: String
        "z" @ 81..82: Variable
        "/"outer_local/"" @ 85..98: String
        "inner" @ 108..113: Function [definition]
        "x" @ 134..135: Variable
        "z" @ 137..138: Variable
        "y" @ 189..190: Variable
        "x" @ 239..240: Variable
        "/"modified/"" @ 243..253: String
        "y" @ 262..263: Variable
        "/"modified_global/"" @ 266..283: String
        "z" @ 292..293: Variable
        "/"modified_local/"" @ 296..312: String
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
    fn test_nonlocal_global_edge_cases() {
        let test = cursor_test(
            r#"
# Single variable statements
def test():
    global x
    nonlocal y
    
    # Multiple variables in one statement
    global a, b, c
    nonlocal d, e, f
    
    return x + y + a + b + c + d + e + f<CURSOR>
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
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
    fn test_pattern_matching() {
        let test = cursor_test(
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
            return fallback<CURSOR>
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "process_data" @ 5..17: Function [definition]
        "data" @ 18..22: Parameter
        "data" @ 35..39: Variable
        "/"name/"" @ 55..61: String
        "name" @ 63..67: Variable
        "/"age/"" @ 69..74: String
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
    fn test_exception_handlers() {
        let test = cursor_test(
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
    pass<CURSOR>
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "x" @ 10..11: Variable
        "1" @ 14..15: Number
        "0" @ 18..19: Number
        "ValueError" @ 27..37: Class
        "ve" @ 41..43: Variable
        "print" @ 49..54: Function
        "ve" @ 55..57: Variable
        "TypeError" @ 67..76: Class
        "RuntimeError" @ 78..90: Class
        "re" @ 95..97: Variable
        "print" @ 103..108: Function
        "re" @ 109..111: Variable
        "Exception" @ 120..129: Class
        "e" @ 133..134: Variable
        "print" @ 140..145: Function
        "e" @ 146..147: Variable
        "#);
    }

    #[test]
    fn test_self_attribute_expression() {
        let test = cursor_test(
            r#"
from typing import Self


class C:
    def __init__(self: Self):
        self.annotated: int = 1
        self.non_annotated = 1
        self.x.test()
        self.x()<CURSOR>


"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "typing" @ 6..12: Namespace
        "Self" @ 20..24: Variable
        "C" @ 33..34: Class [definition]
        "__init__" @ 44..52: Method [definition]
        "self" @ 53..57: SelfParameter
        "Self" @ 59..63: TypeParameter
        "self" @ 74..78: Parameter
        "annotated" @ 79..88: Variable
        "int" @ 90..93: Class
        "1" @ 96..97: Number
        "self" @ 106..110: Parameter
        "non_annotated" @ 111..124: Variable
        "1" @ 127..128: Number
        "self" @ 137..141: Parameter
        "x" @ 142..143: Variable
        "test" @ 144..148: Variable
        "self" @ 159..163: Parameter
        "x" @ 164..165: Variable
        "#);
    }

    /// Regression test for <https://github.com/astral-sh/ty/issues/1406>
    #[test]
    fn test_invalid_kwargs() {
        let test = cursor_test(
            r#"
def foo(self, **<CURSOR>key, value=10):
    return
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_snapshot!(semantic_tokens_to_snapshot(&test.db, test.cursor.file, &tokens), @r#"
        "foo" @ 5..8: Function [definition]
        "self" @ 9..13: Parameter
        "key" @ 17..20: Parameter
        "value" @ 22..27: Parameter
        "10" @ 28..30: Number
        "#);
    }
}
