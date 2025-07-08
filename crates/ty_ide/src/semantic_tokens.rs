use crate::Db;
use bitflags::bitflags;
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
    HasType, SemanticModel,
    semantic_index::definition::DefinitionKind,
    types::{Type, definition_kind_for_name},
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

    fn visit_type_annotation(&mut self, annotation: &ast::Expr) {
        let prev_in_type_annotation = self.in_type_annotation;
        self.in_type_annotation = true;
        self.visit_expr(annotation);
        self.in_type_annotation = prev_in_type_annotation;
    }

    // Visit parameters for a function or lambda expression and classify
    // them as parameters, selfParameter, or clsParameter as appropriate.
    fn visit_parameters(
        &mut self,
        parameters: &ast::Parameters,
        func: Option<&ast::StmtFunctionDef>,
    ) {
        // Parameters
        for (i, param) in parameters.args.iter().enumerate() {
            let token_type = if let Some(func) = func {
                // For function definitions, use the classification logic to determine
                // whether this is a self/cls parameter or just a regular parameter
                self.classify_parameter(&param.parameter, i == 0, func)
            } else {
                // For lambdas, all parameters are just parameters (no self/cls)
                SemanticTokenType::Parameter
            };

            self.add_token(
                param.parameter.name.range(),
                token_type,
                SemanticTokenModifier::empty(),
            );

            // Handle parameter type annotations
            if let Some(annotation) = &param.parameter.annotation {
                self.visit_type_annotation(annotation);
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
                    self.visit_type_annotation(returns);
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
            ast::Stmt::AnnAssign(assign) => {
                // Handle annotated assignments (e.g., x: int = 5)
                if let ast::Expr::Name(name) = assign.target.as_ref() {
                    let (token_type, modifiers) = self.classify_name(name);
                    self.add_token(name, token_type, modifiers);
                }

                // Handle the type annotation
                self.visit_type_annotation(&assign.annotation);

                // Handle the value if present
                if let Some(value) = &assign.value {
                    self.visit_expr(value);
                }
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
            _ => {
                // For all other statement types, let the default visitor handle them
                walk_stmt(self, stmt);
            }
        }
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
        match type_param {
            TypeParam::TypeVar(type_var) => {
                if let Some(bound) = &type_var.bound {
                    self.visit_type_annotation(bound);
                }
                if let Some(default) = &type_var.default {
                    self.visit_type_annotation(default);
                }
            }
            TypeParam::ParamSpec(param_spec) => {
                if let Some(default) = &param_spec.default {
                    self.visit_type_annotation(default);
                }
            }
            TypeParam::TypeVarTuple(type_var_tuple) => {
                if let Some(default) = &type_var_tuple.default {
                    self.visit_type_annotation(default);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::{
        diagnostic::{
            Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig,
            LintName, Severity, Span,
        },
        files::FileRange,
    };
    use ruff_text_size::TextSize;

    #[test]
    fn test_semantic_tokens_basic() {
        let test = cursor_test("def foo(): pass<CURSOR>");

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: function
         --> main.py:1:5
          |
        1 | def foo(): pass
          |     ^^^ DEFINITION
          |
        ");
    }

    #[test]
    fn test_semantic_tokens_class() {
        let test = cursor_test("class MyClass: pass<CURSOR>");

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:1:7
          |
        1 | class MyClass: pass
          |       ^^^^^^^ DEFINITION
          |
        ");
    }

    #[test]
    fn test_semantic_tokens_variables() {
        let test = cursor_test(
            "
x = 42
y = 'hello'<CURSOR>
",
        );

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: variable
         --> main.py:2:1
          |
        2 | x = 42
          | ^
          |

        info[semantic-token]: number
         --> main.py:2:5
          |
        2 | x = 42
          |     ^^
          |

        info[semantic-token]: variable
         --> main.py:3:1
          |
        3 | y = 'hello'
          | ^
          |

        info[semantic-token]: string
         --> main.py:3:5
          |
        3 | y = 'hello'
          |     ^^^^^^^
          |
        ");
    }

    #[test]
    fn test_semantic_tokens_self_parameter() {
        let test = cursor_test(
            "
class MyClass:
    def method(self, x): pass<CURSOR>
",
        );

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: method
         --> main.py:3:9
          |
        3 |     def method(self, x): pass
          |         ^^^^^^ DEFINITION
          |

        info[semantic-token]: selfParameter
         --> main.py:3:16
          |
        3 |     def method(self, x): pass
          |                ^^^^
          |

        info[semantic-token]: parameter
         --> main.py:3:22
          |
        3 |     def method(self, x): pass
          |                      ^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: decorator
         --> main.py:3:6
          |
        3 |     @classmethod
          |      ^^^^^^^^^^^
          |

        info[semantic-token]: method
         --> main.py:4:9
          |
        4 |     def method(cls, x): pass
          |         ^^^^^^ DEFINITION
          |

        info[semantic-token]: clsParameter
         --> main.py:4:16
          |
        4 |     def method(cls, x): pass
          |                ^^^
          |

        info[semantic-token]: parameter
         --> main.py:4:21
          |
        4 |     def method(cls, x): pass
          |                     ^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: decorator
         --> main.py:3:6
          |
        3 |     @staticmethod
          |      ^^^^^^^^^^^^
          |

        info[semantic-token]: method
         --> main.py:4:9
          |
        4 |     def method(x, y): pass
          |         ^^^^^^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:4:16
          |
        4 |     def method(x, y): pass
          |                ^
          |

        info[semantic-token]: parameter
         --> main.py:4:19
          |
        4 |     def method(x, y): pass
          |                   ^
          |
        ");
    }

    #[test]
    fn test_semantic_tokens_custom_self_cls_names() {
        let test = cursor_test(
            "
class MyClass:
    def method(instance, x): pass
    @classmethod
    def other(klass, y): pass<CURSOR>
",
        );

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: method
         --> main.py:3:9
          |
        3 |     def method(instance, x): pass
          |         ^^^^^^ DEFINITION
          |

        info[semantic-token]: selfParameter
         --> main.py:3:16
          |
        3 |     def method(instance, x): pass
          |                ^^^^^^^^
          |

        info[semantic-token]: parameter
         --> main.py:3:26
          |
        3 |     def method(instance, x): pass
          |                          ^
          |

        info[semantic-token]: decorator
         --> main.py:4:6
          |
        4 |     @classmethod
          |      ^^^^^^^^^^^
          |

        info[semantic-token]: method
         --> main.py:5:9
          |
        5 |     def other(klass, y): pass
          |         ^^^^^ DEFINITION
          |

        info[semantic-token]: clsParameter
         --> main.py:5:15
          |
        5 |     def other(klass, y): pass
          |               ^^^^^
          |

        info[semantic-token]: parameter
         --> main.py:5:22
          |
        5 |     def other(klass, y): pass
          |                      ^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:3:5
          |
        3 |     CONSTANT = 42
          |     ^^^^^^^^ READONLY
          |

        info[semantic-token]: number
         --> main.py:3:16
          |
        3 |     CONSTANT = 42
          |                ^^
          |

        info[semantic-token]: method
         --> main.py:4:15
          |
        4 |     async def method(self): pass
          |               ^^^^^^ DEFINITION, ASYNC
          |

        info[semantic-token]: selfParameter
         --> main.py:4:22
          |
        4 |     async def method(self): pass
          |                      ^^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: namespace
         --> main.py:2:8
          |
        2 | import sys
          |        ^^^
          |

        info[semantic-token]: class
         --> main.py:3:7
          |
        3 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: function
         --> main.py:6:5
          |
        6 | def my_function():
          |     ^^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: number
         --> main.py:7:12
          |
        7 |     return 42
          |            ^^
          |

        info[semantic-token]: variable
         --> main.py:9:1
          |
        9 | x = MyClass()
          | ^
          |

        info[semantic-token]: class
         --> main.py:9:5
          |
        9 | x = MyClass()
          |     ^^^^^^^
          |

        info[semantic-token]: variable
          --> main.py:10:1
           |
        10 | y = my_function()
           | ^
           |

        info[semantic-token]: function
          --> main.py:10:5
           |
        10 | y = my_function()
           |     ^^^^^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:11:1
           |
        11 | z = sys.version
           | ^
           |

        info[semantic-token]: namespace
          --> main.py:11:5
           |
        11 | z = sys.version
           |     ^^^
           |

        info[semantic-token]: variable
          --> main.py:11:9
           |
        11 | z = sys.version
           |         ^^^^^^^
           |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: variable
         --> main.py:2:1
          |
        2 | x = True
          | ^
          |

        info[semantic-token]: builtinConstant
         --> main.py:2:5
          |
        2 | x = True
          |     ^^^^
          |

        info[semantic-token]: variable
         --> main.py:3:1
          |
        3 | y = False
          | ^
          |

        info[semantic-token]: builtinConstant
         --> main.py:3:5
          |
        3 | y = False
          |     ^^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:1
          |
        4 | z = None
          | ^
          |

        info[semantic-token]: builtinConstant
         --> main.py:4:5
          |
        4 | z = None
          |     ^^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: function
         --> main.py:2:5
          |
        2 | def check(value):
          |     ^^^^^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:2:11
          |
        2 | def check(value):
          |           ^^^^^
          |

        info[semantic-token]: variable
         --> main.py:3:8
          |
        3 |     if value is None:
          |        ^^^^^
          |

        info[semantic-token]: builtinConstant
         --> main.py:3:17
          |
        3 |     if value is None:
          |                 ^^^^
          |

        info[semantic-token]: builtinConstant
         --> main.py:4:16
          |
        4 |         return False
          |                ^^^^^
          |

        info[semantic-token]: builtinConstant
         --> main.py:5:12
          |
        5 |     return True
          |            ^^^^
          |

        info[semantic-token]: variable
         --> main.py:7:1
          |
        7 | result = check(None)
          | ^^^^^^
          |

        info[semantic-token]: function
         --> main.py:7:10
          |
        7 | result = check(None)
          |          ^^^^^
          |

        info[semantic-token]: builtinConstant
         --> main.py:7:16
          |
        7 | result = check(None)
          |                ^^^^
          |
        ");
    }

    #[test]
    fn test_semantic_tokens_range() {
        let source = "
def function1():
    x = 42
    return x

def function2():
    y = \"hello\"
    z = True
    return y + z<CURSOR>
";

        let test = cursor_test(source);

        let full_tokens = semantic_tokens(&test.db, test.cursor.file, None);

        // Get the range that covers only the second function
        let range = ruff_text_size::TextRange::new(
            TextSize::try_from(source.find("def function2()").unwrap()).unwrap(),
            source.text_len(),
        );

        let range_tokens = semantic_tokens(&test.db, test.cursor.file, Some(range));

        // Range-based tokens should have fewer tokens than full scan
        // (should exclude tokens from function1)
        assert!(range_tokens.len() < full_tokens.len());

        // Test both full tokens and range tokens with snapshots
        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: function
         --> main.py:2:5
          |
        2 | def function1():
          |     ^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:3:5
          |
        3 |     x = 42
          |     ^
          |

        info[semantic-token]: number
         --> main.py:3:9
          |
        3 |     x = 42
          |         ^^
          |

        info[semantic-token]: variable
         --> main.py:4:12
          |
        4 |     return x
          |            ^
          |

        info[semantic-token]: function
         --> main.py:6:5
          |
        6 | def function2():
          |     ^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:7:5
          |
        7 |     y = "hello"
          |     ^
          |

        info[semantic-token]: string
         --> main.py:7:9
          |
        7 |     y = "hello"
          |         ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:8:5
          |
        8 |     z = True
          |     ^
          |

        info[semantic-token]: builtinConstant
         --> main.py:8:9
          |
        8 |     z = True
          |         ^^^^
          |

        info[semantic-token]: variable
         --> main.py:9:12
          |
        9 |     return y + z
          |            ^
          |

        info[semantic-token]: variable
         --> main.py:9:16
          |
        9 |     return y + z
          |                ^
          |
        "#);

        assert_snapshot!(test.semantic_tokens(Some(range)), @r#"
        info[semantic-token]: function
         --> main.py:6:5
          |
        6 | def function2():
          |     ^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:7:5
          |
        7 |     y = "hello"
          |     ^
          |

        info[semantic-token]: string
         --> main.py:7:9
          |
        7 |     y = "hello"
          |         ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:8:5
          |
        8 |     z = True
          |     ^
          |

        info[semantic-token]: builtinConstant
         --> main.py:8:9
          |
        8 |     z = True
          |         ^^^^
          |

        info[semantic-token]: variable
         --> main.py:9:12
          |
        9 |     return y + z
          |            ^
          |

        info[semantic-token]: variable
         --> main.py:9:16
          |
        9 |     return y + z
          |                ^
          |
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: namespace
         --> main.py:2:8
          |
        2 | import os.path
          |        ^^
          |

        info[semantic-token]: namespace
         --> main.py:2:11
          |
        2 | import os.path
          |           ^^^^
          |

        info[semantic-token]: namespace
         --> main.py:3:8
          |
        3 | import sys.version_info
          |        ^^^
          |

        info[semantic-token]: namespace
         --> main.py:3:12
          |
        3 | import sys.version_info
          |            ^^^^^^^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:4:6
          |
        4 | from urllib.parse import urlparse
          |      ^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:4:13
          |
        4 | from urllib.parse import urlparse
          |             ^^^^^
          |

        info[semantic-token]: function
         --> main.py:4:26
          |
        4 | from urllib.parse import urlparse
          |                          ^^^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:5:6
          |
        5 | from collections.abc import Mapping
          |      ^^^^^^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:5:18
          |
        5 | from collections.abc import Mapping
          |                  ^^^
          |

        info[semantic-token]: class
         --> main.py:5:29
          |
        5 | from collections.abc import Mapping
          |                             ^^^^^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: namespace
         --> main.py:2:8
          |
        2 | import os
          |        ^^
          |

        info[semantic-token]: namespace
         --> main.py:3:8
          |
        3 | import sys
          |        ^^^
          |

        info[semantic-token]: namespace
         --> main.py:4:6
          |
        4 | from collections import defaultdict
          |      ^^^^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:4:25
          |
        4 | from collections import defaultdict
          |                         ^^^^^^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:7:1
          |
        7 | x = os
          | ^
          |

        info[semantic-token]: namespace
         --> main.py:7:5
          |
        7 | x = os
          |     ^^
          |

        info[semantic-token]: namespace
         --> main.py:8:1
          |
        8 | y = sys
          | ^
          |

        info[semantic-token]: namespace
         --> main.py:8:5
          |
        8 | y = sys
          |     ^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: namespace
         --> main.py:2:6
          |
        2 | from os import path
          |      ^^
          |

        info[semantic-token]: namespace
         --> main.py:2:16
          |
        2 | from os import path
          |                ^^^^
          |

        info[semantic-token]: namespace
         --> main.py:3:6
          |
        3 | from collections import defaultdict, OrderedDict, Counter
          |      ^^^^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:3:25
          |
        3 | from collections import defaultdict, OrderedDict, Counter
          |                         ^^^^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:3:38
          |
        3 | from collections import defaultdict, OrderedDict, Counter
          |                                      ^^^^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:3:51
          |
        3 | from collections import defaultdict, OrderedDict, Counter
          |                                                   ^^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:4:6
          |
        4 | from typing import List, Dict, Optional
          |      ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:20
          |
        4 | from typing import List, Dict, Optional
          |                    ^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:26
          |
        4 | from typing import List, Dict, Optional
          |                          ^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:32
          |
        4 | from typing import List, Dict, Optional
          |                                ^^^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:5:6
          |
        5 | from mymodule import CONSTANT, my_function, MyClass
          |      ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:22
          |
        5 | from mymodule import CONSTANT, my_function, MyClass
          |                      ^^^^^^^^ READONLY
          |

        info[semantic-token]: variable
         --> main.py:5:32
          |
        5 | from mymodule import CONSTANT, my_function, MyClass
          |                                ^^^^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:45
          |
        5 | from mymodule import CONSTANT, my_function, MyClass
          |                                             ^^^^^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: namespace
         --> main.py:2:8
          |
        2 | import os
          |        ^^
          |

        info[semantic-token]: namespace
         --> main.py:3:8
          |
        3 | import sys
          |        ^^^
          |

        info[semantic-token]: namespace
         --> main.py:4:6
          |
        4 | from collections import defaultdict
          |      ^^^^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:4:25
          |
        4 | from collections import defaultdict
          |                         ^^^^^^^^^^^
          |

        info[semantic-token]: namespace
         --> main.py:5:6
          |
        5 | from typing import List
          |      ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:20
          |
        5 | from typing import List
          |                    ^^^^
          |

        info[semantic-token]: class
         --> main.py:7:7
          |
        7 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:8:5
          |
        8 |     CONSTANT = 42
          |     ^^^^^^^^ READONLY
          |

        info[semantic-token]: number
         --> main.py:8:16
          |
        8 |     CONSTANT = 42
          |                ^^
          |

        info[semantic-token]: method
          --> main.py:10:9
           |
        10 |     def method(self):
           |         ^^^^^^ DEFINITION
           |

        info[semantic-token]: selfParameter
          --> main.py:10:16
           |
        10 |     def method(self):
           |                ^^^^
           |

        info[semantic-token]: string
          --> main.py:11:16
           |
        11 |         return "hello"
           |                ^^^^^^^
           |

        info[semantic-token]: decorator
          --> main.py:13:6
           |
        13 |     @property
           |      ^^^^^^^^
           |

        info[semantic-token]: method
          --> main.py:14:9
           |
        14 |     def prop(self):
           |         ^^^^ DEFINITION
           |

        info[semantic-token]: selfParameter
          --> main.py:14:14
           |
        14 |     def prop(self):
           |              ^^^^
           |

        info[semantic-token]: variable
          --> main.py:15:16
           |
        15 |         return self.CONSTANT
           |                ^^^^
           |

        info[semantic-token]: variable
          --> main.py:15:21
           |
        15 |         return self.CONSTANT
           |                     ^^^^^^^^ READONLY
           |

        info[semantic-token]: variable
          --> main.py:17:1
           |
        17 | obj = MyClass()
           | ^^^
           |

        info[semantic-token]: class
          --> main.py:17:7
           |
        17 | obj = MyClass()
           |       ^^^^^^^
           |

        info[semantic-token]: namespace
          --> main.py:20:1
           |
        20 | x = os.path              # path should be namespace (module)
           | ^
           |

        info[semantic-token]: namespace
          --> main.py:20:5
           |
        20 | x = os.path              # path should be namespace (module)
           |     ^^
           |

        info[semantic-token]: namespace
          --> main.py:20:8
           |
        20 | x = os.path              # path should be namespace (module)
           |        ^^^^
           |

        info[semantic-token]: method
          --> main.py:21:1
           |
        21 | y = obj.method           # method should be method (bound method)
           | ^
           |

        info[semantic-token]: variable
          --> main.py:21:5
           |
        21 | y = obj.method           # method should be method (bound method)
           |     ^^^
           |

        info[semantic-token]: method
          --> main.py:21:9
           |
        21 | y = obj.method           # method should be method (bound method)
           |         ^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:22:1
           |
        22 | z = obj.CONSTANT         # CONSTANT should be variable with readonly modifier
           | ^
           |

        info[semantic-token]: variable
          --> main.py:22:5
           |
        22 | z = obj.CONSTANT         # CONSTANT should be variable with readonly modifier
           |     ^^^
           |

        info[semantic-token]: variable
          --> main.py:22:9
           |
        22 | z = obj.CONSTANT         # CONSTANT should be variable with readonly modifier
           |         ^^^^^^^^ READONLY
           |

        info[semantic-token]: variable
          --> main.py:23:1
           |
        23 | w = obj.prop             # prop should be property
           | ^
           |

        info[semantic-token]: variable
          --> main.py:23:5
           |
        23 | w = obj.prop             # prop should be property
           |     ^^^
           |

        info[semantic-token]: variable
          --> main.py:23:9
           |
        23 | w = obj.prop             # prop should be property
           |         ^^^^
           |

        info[semantic-token]: function
          --> main.py:24:1
           |
        24 | v = MyClass.method       # method should be method (function)
           | ^
           |

        info[semantic-token]: class
          --> main.py:24:5
           |
        24 | v = MyClass.method       # method should be method (function)
           |     ^^^^^^^
           |

        info[semantic-token]: method
          --> main.py:24:13
           |
        24 | v = MyClass.method       # method should be method (function)
           |             ^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:25:1
           |
        25 | u = List.__name__        # __name__ should be variable
           | ^
           |

        info[semantic-token]: variable
          --> main.py:25:5
           |
        25 | u = List.__name__        # __name__ should be variable
           |     ^^^^
           |

        info[semantic-token]: variable
          --> main.py:25:10
           |
        25 | u = List.__name__        # __name__ should be variable
           |          ^^^^^^^^
           |
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:3:5
          |
        3 |     some_attr = "value"
          |     ^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:3:17
          |
        3 |     some_attr = "value"
          |                 ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:1
          |
        5 | obj = MyClass()
          | ^^^
          |

        info[semantic-token]: class
         --> main.py:5:7
          |
        5 | obj = MyClass()
          |       ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:7:1
          |
        7 | x = obj.some_attr        # Should fall back to variable, not property
          | ^
          |

        info[semantic-token]: variable
         --> main.py:7:5
          |
        7 | x = obj.some_attr        # Should fall back to variable, not property
          |     ^^^
          |

        info[semantic-token]: variable
         --> main.py:7:9
          |
        7 | x = obj.some_attr        # Should fall back to variable, not property
          |         ^^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:8:1
          |
        8 | y = obj.unknown_attr     # Should fall back to variable
          | ^
          |

        info[semantic-token]: variable
         --> main.py:8:5
          |
        8 | y = obj.unknown_attr     # Should fall back to variable
          |     ^^^
          |

        info[semantic-token]: variable
         --> main.py:8:9
          |
        8 | y = obj.unknown_attr     # Should fall back to variable
          |         ^^^^^^^^^^^^
          |
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:3:5
          |
        3 |     UPPER_CASE = 42
          |     ^^^^^^^^^^ READONLY
          |

        info[semantic-token]: number
         --> main.py:3:18
          |
        3 |     UPPER_CASE = 42
          |                  ^^
          |

        info[semantic-token]: variable
         --> main.py:4:5
          |
        4 |     lower_case = 24
          |     ^^^^^^^^^^
          |

        info[semantic-token]: number
         --> main.py:4:18
          |
        4 |     lower_case = 24
          |                  ^^
          |

        info[semantic-token]: variable
         --> main.py:5:5
          |
        5 |     MixedCase = 12
          |     ^^^^^^^^^
          |

        info[semantic-token]: number
         --> main.py:5:17
          |
        5 |     MixedCase = 12
          |                 ^^
          |

        info[semantic-token]: variable
         --> main.py:6:5
          |
        6 |     A = 1
          |     ^
          |

        info[semantic-token]: number
         --> main.py:6:9
          |
        6 |     A = 1
          |         ^
          |

        info[semantic-token]: variable
         --> main.py:8:1
          |
        8 | obj = MyClass()
          | ^^^
          |

        info[semantic-token]: class
         --> main.py:8:7
          |
        8 | obj = MyClass()
          |       ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:9:1
          |
        9 | x = obj.UPPER_CASE    # Should have readonly modifier
          | ^
          |

        info[semantic-token]: variable
         --> main.py:9:5
          |
        9 | x = obj.UPPER_CASE    # Should have readonly modifier
          |     ^^^
          |

        info[semantic-token]: variable
         --> main.py:9:9
          |
        9 | x = obj.UPPER_CASE    # Should have readonly modifier
          |         ^^^^^^^^^^ READONLY
          |

        info[semantic-token]: variable
          --> main.py:10:1
           |
        10 | y = obj.lower_case    # Should not have readonly modifier
           | ^
           |

        info[semantic-token]: variable
          --> main.py:10:5
           |
        10 | y = obj.lower_case    # Should not have readonly modifier
           |     ^^^
           |

        info[semantic-token]: variable
          --> main.py:10:9
           |
        10 | y = obj.lower_case    # Should not have readonly modifier
           |         ^^^^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:11:1
           |
        11 | z = obj.MixedCase     # Should not have readonly modifier
           | ^
           |

        info[semantic-token]: variable
          --> main.py:11:5
           |
        11 | z = obj.MixedCase     # Should not have readonly modifier
           |     ^^^
           |

        info[semantic-token]: variable
          --> main.py:11:9
           |
        11 | z = obj.MixedCase     # Should not have readonly modifier
           |         ^^^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:12:1
           |
        12 | w = obj.A             # Should not have readonly modifier (length == 1)
           | ^
           |

        info[semantic-token]: variable
          --> main.py:12:5
           |
        12 | w = obj.A             # Should not have readonly modifier (length == 1)
           |     ^^^
           |

        info[semantic-token]: variable
          --> main.py:12:9
           |
        12 | w = obj.A             # Should not have readonly modifier (length == 1)
           |         ^
           |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: namespace
         --> main.py:2:6
          |
        2 | from typing import List, Optional
          |      ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:20
          |
        2 | from typing import List, Optional
          |                    ^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:26
          |
        2 | from typing import List, Optional
          |                          ^^^^^^^^
          |

        info[semantic-token]: function
         --> main.py:4:5
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:4:31
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |                               ^^^^^^
          |

        info[semantic-token]: class
         --> main.py:4:39
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |                                       ^^^
          |

        info[semantic-token]: parameter
         --> main.py:4:44
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |                                            ^^^^^^
          |

        info[semantic-token]: class
         --> main.py:4:52
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |                                                    ^^^
          |

        info[semantic-token]: variable
         --> main.py:4:60
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |                                                            ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:69
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |                                                                     ^^^^
          |

        info[semantic-token]: class
         --> main.py:4:74
          |
        4 | def function_with_annotations(param1: int, param2: str) -> Optional[List[str]]:
          |                                                                          ^^^
          |

        info[semantic-token]: variable
         --> main.py:7:1
          |
        7 | x: int = 42
          | ^
          |

        info[semantic-token]: class
         --> main.py:7:4
          |
        7 | x: int = 42
          |    ^^^
          |

        info[semantic-token]: number
         --> main.py:7:10
          |
        7 | x: int = 42
          |          ^^
          |

        info[semantic-token]: variable
         --> main.py:8:1
          |
        8 | y: Optional[str] = None
          | ^
          |

        info[semantic-token]: variable
         --> main.py:8:4
          |
        8 | y: Optional[str] = None
          |    ^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:8:13
          |
        8 | y: Optional[str] = None
          |             ^^^
          |

        info[semantic-token]: builtinConstant
         --> main.py:8:20
          |
        8 | y: Optional[str] = None
          |                    ^^^^
          |
        ");
    }

    #[test]
    fn test_debug_int_classification() {
        let test = cursor_test(
            "
x: int = 42<CURSOR>
",
        );

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: variable
         --> main.py:2:1
          |
        2 | x: int = 42
          | ^
          |

        info[semantic-token]: class
         --> main.py:2:4
          |
        2 | x: int = 42
          |    ^^^
          |

        info[semantic-token]: number
         --> main.py:2:10
          |
        2 | x: int = 42
          |          ^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: class
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:5:1
          |
        5 | x: MyClass = MyClass()
          | ^
          |

        info[semantic-token]: class
         --> main.py:5:4
          |
        5 | x: MyClass = MyClass()
          |    ^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:5:14
          |
        5 | x: MyClass = MyClass()
          |              ^^^^^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: namespace
         --> main.py:2:6
          |
        2 | from typing import List, Optional
          |      ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:20
          |
        2 | from typing import List, Optional
          |                    ^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:26
          |
        2 | from typing import List, Optional
          |                          ^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:4:7
          |
        4 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |

        info[semantic-token]: function
         --> main.py:7:5
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |     ^^^^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:7:19
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |                   ^^^^^
          |

        info[semantic-token]: class
         --> main.py:7:26
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |                          ^^^
          |

        info[semantic-token]: parameter
         --> main.py:7:31
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |                               ^^^^^
          |

        info[semantic-token]: class
         --> main.py:7:38
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |                                      ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:7:50
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |                                                  ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:7:59
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |                                                           ^^^^
          |

        info[semantic-token]: class
         --> main.py:7:64
          |
        7 | def test_function(param: int, other: MyClass) -> Optional[List[str]]:
          |                                                                ^^^
          |

        info[semantic-token]: variable
         --> main.py:9:5
          |
        9 |     x: int = 42
          |     ^
          |

        info[semantic-token]: class
         --> main.py:9:8
          |
        9 |     x: int = 42
          |        ^^^
          |

        info[semantic-token]: number
         --> main.py:9:14
          |
        9 |     x: int = 42
          |              ^^
          |

        info[semantic-token]: variable
          --> main.py:10:5
           |
        10 |     y: MyClass = MyClass()
           |     ^
           |

        info[semantic-token]: class
          --> main.py:10:8
           |
        10 |     y: MyClass = MyClass()
           |        ^^^^^^^
           |

        info[semantic-token]: class
          --> main.py:10:18
           |
        10 |     y: MyClass = MyClass()
           |                  ^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:11:5
           |
        11 |     z: List[str] = ["hello"]
           |     ^
           |

        info[semantic-token]: variable
          --> main.py:11:8
           |
        11 |     z: List[str] = ["hello"]
           |        ^^^^
           |

        info[semantic-token]: class
          --> main.py:11:13
           |
        11 |     z: List[str] = ["hello"]
           |             ^^^
           |

        info[semantic-token]: string
          --> main.py:11:21
           |
        11 |     z: List[str] = ["hello"]
           |                     ^^^^^^^
           |

        info[semantic-token]: builtinConstant
          --> main.py:15:12
           |
        15 |     return None
           |            ^^^^
           |
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: namespace
         --> main.py:2:6
          |
        2 | from typing import Protocol
          |      ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:20
          |
        2 | from typing import Protocol
          |                    ^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:4:7
          |
        4 | class MyProtocol(Protocol):
          |       ^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:4:18
          |
        4 | class MyProtocol(Protocol):
          |                  ^^^^^^^^
          |

        info[semantic-token]: method
         --> main.py:5:9
          |
        5 |     def method(self) -> int: ...
          |         ^^^^^^ DEFINITION
          |

        info[semantic-token]: selfParameter
         --> main.py:5:16
          |
        5 |     def method(self) -> int: ...
          |                ^^^^
          |

        info[semantic-token]: class
         --> main.py:5:25
          |
        5 |     def method(self) -> int: ...
          |                         ^^^
          |

        info[semantic-token]: function
         --> main.py:7:5
          |
        7 | def test_function(param: MyProtocol) -> None:
          |     ^^^^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:7:19
          |
        7 | def test_function(param: MyProtocol) -> None:
          |                   ^^^^^
          |

        info[semantic-token]: class
         --> main.py:7:26
          |
        7 | def test_function(param: MyProtocol) -> None:
          |                          ^^^^^^^^^^
          |

        info[semantic-token]: builtinConstant
         --> main.py:7:41
          |
        7 | def test_function(param: MyProtocol) -> None:
          |                                         ^^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: namespace
         --> main.py:2:6
          |
        2 | from typing import Protocol
          |      ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:20
          |
        2 | from typing import Protocol
          |                    ^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:4:7
          |
        4 | class MyProtocol(Protocol):
          |       ^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: variable
         --> main.py:4:18
          |
        4 | class MyProtocol(Protocol):
          |                  ^^^^^^^^
          |

        info[semantic-token]: method
         --> main.py:5:9
          |
        5 |     def method(self) -> int: ...
          |         ^^^^^^ DEFINITION
          |

        info[semantic-token]: selfParameter
         --> main.py:5:16
          |
        5 |     def method(self) -> int: ...
          |                ^^^^
          |

        info[semantic-token]: class
         --> main.py:5:25
          |
        5 |     def method(self) -> int: ...
          |                         ^^^
          |

        info[semantic-token]: class
         --> main.py:8:1
          |
        8 | my_protocol_var = MyProtocol
          | ^^^^^^^^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:8:19
          |
        8 | my_protocol_var = MyProtocol
          |                   ^^^^^^^^^^
          |

        info[semantic-token]: function
          --> main.py:11:5
           |
        11 | def test_function(param: MyProtocol) -> MyProtocol:
           |     ^^^^^^^^^^^^^ DEFINITION
           |

        info[semantic-token]: parameter
          --> main.py:11:19
           |
        11 | def test_function(param: MyProtocol) -> MyProtocol:
           |                   ^^^^^
           |

        info[semantic-token]: class
          --> main.py:11:26
           |
        11 | def test_function(param: MyProtocol) -> MyProtocol:
           |                          ^^^^^^^^^^
           |

        info[semantic-token]: class
          --> main.py:11:41
           |
        11 | def test_function(param: MyProtocol) -> MyProtocol:
           |                                         ^^^^^^^^^^
           |

        info[semantic-token]: parameter
          --> main.py:12:12
           |
        12 |     return param
           |            ^^^^^
           |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: function
         --> main.py:5:5
          |
        5 | def func[T](x: T) -> T:
          |     ^^^^ DEFINITION
          |

        info[semantic-token]: typeParameter
         --> main.py:5:10
          |
        5 | def func[T](x: T) -> T:
          |          ^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:5:13
          |
        5 | def func[T](x: T) -> T:
          |             ^
          |

        info[semantic-token]: typeParameter
         --> main.py:5:16
          |
        5 | def func[T](x: T) -> T:
          |                ^
          |

        info[semantic-token]: typeParameter
         --> main.py:5:22
          |
        5 | def func[T](x: T) -> T:
          |                      ^
          |

        info[semantic-token]: parameter
         --> main.py:6:12
          |
        6 |     return x
          |            ^
          |

        info[semantic-token]: function
         --> main.py:9:5
          |
        9 | def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
          |     ^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: typeParameter
         --> main.py:9:17
          |
        9 | def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
          |                 ^^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:9:21
          |
        9 | def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
          |                     ^^^^
          |

        info[semantic-token]: class
         --> main.py:9:27
          |
        9 | def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
          |                           ^^^^^
          |

        info[semantic-token]: variable
         --> main.py:9:34
          |
        9 | def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
          |                                  ^^
          |

        info[semantic-token]: class
         --> main.py:9:42
          |
        9 | def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
          |                                          ^^^^^
          |

        info[semantic-token]: variable
         --> main.py:9:49
          |
        9 | def func_tuple[*Ts](args: tuple[*Ts]) -> tuple[*Ts]:
          |                                                 ^^
          |

        info[semantic-token]: parameter
          --> main.py:10:12
           |
        10 |     return args
           |            ^^^^
           |

        info[semantic-token]: function
          --> main.py:13:5
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |     ^^^^^^^^^^^^^^ DEFINITION
           |

        info[semantic-token]: typeParameter
          --> main.py:13:22
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                      ^ DEFINITION
           |

        info[semantic-token]: parameter
          --> main.py:13:25
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                         ^^^^
           |

        info[semantic-token]: variable
          --> main.py:13:31
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                               ^^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:13:40
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                                        ^
           |

        info[semantic-token]: class
          --> main.py:13:43
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                                           ^^^
           |

        info[semantic-token]: variable
          --> main.py:13:52
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                                                    ^^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:13:61
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                                                             ^
           |

        info[semantic-token]: class
          --> main.py:13:64
           |
        13 | def func_paramspec[**P](func: Callable[P, int]) -> Callable[P, str]:
           |                                                                ^^^
           |

        info[semantic-token]: function
          --> main.py:14:9
           |
        14 |     def wrapper(*args: P.args, **kwargs: P.kwargs) -> str:
           |         ^^^^^^^ DEFINITION
           |

        info[semantic-token]: class
          --> main.py:14:55
           |
        14 |     def wrapper(*args: P.args, **kwargs: P.kwargs) -> str:
           |                                                       ^^^
           |

        info[semantic-token]: class
          --> main.py:15:16
           |
        15 |         return str(func(*args, **kwargs))
           |                ^^^
           |

        info[semantic-token]: variable
          --> main.py:15:20
           |
        15 |         return str(func(*args, **kwargs))
           |                    ^^^^
           |

        info[semantic-token]: parameter
          --> main.py:15:26
           |
        15 |         return str(func(*args, **kwargs))
           |                          ^^^^
           |

        info[semantic-token]: parameter
          --> main.py:15:34
           |
        15 |         return str(func(*args, **kwargs))
           |                                  ^^^^^^
           |

        info[semantic-token]: function
          --> main.py:16:12
           |
        16 |     return wrapper
           |            ^^^^^^^
           |

        info[semantic-token]: class
          --> main.py:19:7
           |
        19 | class Container[T, U]:
           |       ^^^^^^^^^ DEFINITION
           |

        info[semantic-token]: typeParameter
          --> main.py:19:17
           |
        19 | class Container[T, U]:
           |                 ^ DEFINITION
           |

        info[semantic-token]: typeParameter
          --> main.py:19:20
           |
        19 | class Container[T, U]:
           |                    ^ DEFINITION
           |

        info[semantic-token]: method
          --> main.py:20:9
           |
        20 |     def __init__(self, value1: T, value2: U):
           |         ^^^^^^^^ DEFINITION
           |

        info[semantic-token]: selfParameter
          --> main.py:20:18
           |
        20 |     def __init__(self, value1: T, value2: U):
           |                  ^^^^
           |

        info[semantic-token]: parameter
          --> main.py:20:24
           |
        20 |     def __init__(self, value1: T, value2: U):
           |                        ^^^^^^
           |

        info[semantic-token]: typeParameter
          --> main.py:20:32
           |
        20 |     def __init__(self, value1: T, value2: U):
           |                                ^
           |

        info[semantic-token]: parameter
          --> main.py:20:35
           |
        20 |     def __init__(self, value1: T, value2: U):
           |                                   ^^^^^^
           |

        info[semantic-token]: typeParameter
          --> main.py:20:43
           |
        20 |     def __init__(self, value1: T, value2: U):
           |                                           ^
           |

        info[semantic-token]: typeParameter
          --> main.py:21:22
           |
        21 |         self.value1: T = value1
           |                      ^
           |

        info[semantic-token]: parameter
          --> main.py:21:26
           |
        21 |         self.value1: T = value1
           |                          ^^^^^^
           |

        info[semantic-token]: typeParameter
          --> main.py:22:22
           |
        22 |         self.value2: U = value2
           |                      ^
           |

        info[semantic-token]: parameter
          --> main.py:22:26
           |
        22 |         self.value2: U = value2
           |                          ^^^^^^
           |

        info[semantic-token]: method
          --> main.py:24:9
           |
        24 |     def get_first(self) -> T:
           |         ^^^^^^^^^ DEFINITION
           |

        info[semantic-token]: selfParameter
          --> main.py:24:19
           |
        24 |     def get_first(self) -> T:
           |                   ^^^^
           |

        info[semantic-token]: typeParameter
          --> main.py:24:28
           |
        24 |     def get_first(self) -> T:
           |                            ^
           |

        info[semantic-token]: variable
          --> main.py:25:16
           |
        25 |         return self.value1
           |                ^^^^
           |

        info[semantic-token]: variable
          --> main.py:25:21
           |
        25 |         return self.value1
           |                     ^^^^^^
           |

        info[semantic-token]: method
          --> main.py:27:9
           |
        27 |     def get_second(self) -> U:
           |         ^^^^^^^^^^ DEFINITION
           |

        info[semantic-token]: selfParameter
          --> main.py:27:20
           |
        27 |     def get_second(self) -> U:
           |                    ^^^^
           |

        info[semantic-token]: typeParameter
          --> main.py:27:29
           |
        27 |     def get_second(self) -> U:
           |                             ^
           |

        info[semantic-token]: variable
          --> main.py:28:16
           |
        28 |         return self.value2
           |                ^^^^
           |

        info[semantic-token]: variable
          --> main.py:28:21
           |
        28 |         return self.value2
           |                     ^^^^^^
           |

        info[semantic-token]: class
          --> main.py:31:7
           |
        31 | class BoundedContainer[T: int, U = str]:
           |       ^^^^^^^^^^^^^^^^ DEFINITION
           |

        info[semantic-token]: typeParameter
          --> main.py:31:24
           |
        31 | class BoundedContainer[T: int, U = str]:
           |                        ^ DEFINITION
           |

        info[semantic-token]: class
          --> main.py:31:27
           |
        31 | class BoundedContainer[T: int, U = str]:
           |                           ^^^
           |

        info[semantic-token]: typeParameter
          --> main.py:31:32
           |
        31 | class BoundedContainer[T: int, U = str]:
           |                                ^ DEFINITION
           |

        info[semantic-token]: class
          --> main.py:31:36
           |
        31 | class BoundedContainer[T: int, U = str]:
           |                                    ^^^
           |

        info[semantic-token]: method
          --> main.py:32:9
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |         ^^^^^^^ DEFINITION
           |

        info[semantic-token]: selfParameter
          --> main.py:32:17
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                 ^^^^
           |

        info[semantic-token]: parameter
          --> main.py:32:23
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                       ^
           |

        info[semantic-token]: typeParameter
          --> main.py:32:26
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                          ^
           |

        info[semantic-token]: parameter
          --> main.py:32:29
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                             ^
           |

        info[semantic-token]: typeParameter
          --> main.py:32:32
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                                ^
           |

        info[semantic-token]: class
          --> main.py:32:38
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                                      ^^^^^
           |

        info[semantic-token]: typeParameter
          --> main.py:32:44
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                                            ^
           |

        info[semantic-token]: typeParameter
          --> main.py:32:47
           |
        32 |     def process(self, x: T, y: U) -> tuple[T, U]:
           |                                               ^
           |

        info[semantic-token]: parameter
          --> main.py:33:17
           |
        33 |         return (x, y)
           |                 ^
           |

        info[semantic-token]: parameter
          --> main.py:33:20
           |
        33 |         return (x, y)
           |                    ^
           |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r"
        info[semantic-token]: function
         --> main.py:2:5
          |
        2 | def generic_function[T](value: T) -> T:
          |     ^^^^^^^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: typeParameter
         --> main.py:2:22
          |
        2 | def generic_function[T](value: T) -> T:
          |                      ^ DEFINITION
          |

        info[semantic-token]: parameter
         --> main.py:2:25
          |
        2 | def generic_function[T](value: T) -> T:
          |                         ^^^^^
          |

        info[semantic-token]: typeParameter
         --> main.py:2:32
          |
        2 | def generic_function[T](value: T) -> T:
          |                                ^
          |

        info[semantic-token]: typeParameter
         --> main.py:2:38
          |
        2 | def generic_function[T](value: T) -> T:
          |                                      ^
          |

        info[semantic-token]: variable
         --> main.py:4:5
          |
        4 |     result: T = value
          |     ^^^^^^
          |

        info[semantic-token]: typeParameter
         --> main.py:4:13
          |
        4 |     result: T = value
          |             ^
          |

        info[semantic-token]: parameter
         --> main.py:4:17
          |
        4 |     result: T = value
          |                 ^^^^^
          |

        info[semantic-token]: typeParameter
         --> main.py:5:5
          |
        5 |     temp = result  # This could potentially be T as well
          |     ^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:12
          |
        5 |     temp = result  # This could potentially be T as well
          |            ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:6:12
          |
        6 |     return result
          |            ^^^^^^
          |
        ");
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: decorator
         --> main.py:2:2
          |
        2 | @staticmethod
          |  ^^^^^^^^^^^^
          |

        info[semantic-token]: decorator
         --> main.py:3:2
          |
        3 | @property
          |  ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:2
          |
        4 | @app.route("/path")
          |  ^^^
          |

        info[semantic-token]: variable
         --> main.py:4:6
          |
        4 | @app.route("/path")
          |      ^^^^^
          |

        info[semantic-token]: string
         --> main.py:4:12
          |
        4 | @app.route("/path")
          |            ^^^^^^^
          |

        info[semantic-token]: function
         --> main.py:5:5
          |
        5 | def my_function():
          |     ^^^^^^^^^^^ DEFINITION
          |

        info[semantic-token]: decorator
         --> main.py:8:2
          |
        8 | @dataclass
          |  ^^^^^^^^^
          |

        info[semantic-token]: class
         --> main.py:9:7
          |
        9 | class MyClass:
          |       ^^^^^^^ DEFINITION
          |
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: variable
         --> main.py:1:1
          |
        1 | x = "hello" "world"
          | ^
          |

        info[semantic-token]: string
         --> main.py:1:5
          |
        1 | x = "hello" "world"
          |     ^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:1:13
          |
        1 | x = "hello" "world"
          |             ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:1
          |
        2 | y = ("multi"
          | ^
          |

        info[semantic-token]: string
         --> main.py:2:6
          |
        2 | y = ("multi"
          |      ^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:3:6
          |
        3 |      "line"
          |      ^^^^^^
          |

        info[semantic-token]: string
         --> main.py:4:6
          |
        4 |      "string")
          |      ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:1
          |
        5 | z = 'single' "mixed" 'quotes'
          | ^
          |

        info[semantic-token]: string
         --> main.py:5:5
          |
        5 | z = 'single' "mixed" 'quotes'
          |     ^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:5:14
          |
        5 | z = 'single' "mixed" 'quotes'
          |              ^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:5:22
          |
        5 | z = 'single' "mixed" 'quotes'
          |                      ^^^^^^^^
          |
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: variable
         --> main.py:1:1
          |
        1 | x = b"hello" b"world"
          | ^
          |

        info[semantic-token]: string
         --> main.py:1:5
          |
        1 | x = b"hello" b"world"
          |     ^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:1:14
          |
        1 | x = b"hello" b"world"
          |              ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:2:1
          |
        2 | y = (b"multi"
          | ^
          |

        info[semantic-token]: string
         --> main.py:2:6
          |
        2 | y = (b"multi"
          |      ^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:3:6
          |
        3 |      b"line"
          |      ^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:4:6
          |
        4 |      b"bytes")
          |      ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:1
          |
        5 | z = b'single' b"mixed" b'quotes'
          | ^
          |

        info[semantic-token]: string
         --> main.py:5:5
          |
        5 | z = b'single' b"mixed" b'quotes'
          |     ^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:5:15
          |
        5 | z = b'single' b"mixed" b'quotes'
          |               ^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:5:24
          |
        5 | z = b'single' b"mixed" b'quotes'
          |                        ^^^^^^^^^
          |
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: variable
         --> main.py:2:1
          |
        2 | string_concat = "hello" "world"
          | ^^^^^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:2:17
          |
        2 | string_concat = "hello" "world"
          |                 ^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:2:25
          |
        2 | string_concat = "hello" "world"
          |                         ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:3:1
          |
        3 | bytes_concat = b"hello" b"world"
          | ^^^^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:3:16
          |
        3 | bytes_concat = b"hello" b"world"
          |                ^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:3:25
          |
        3 | bytes_concat = b"hello" b"world"
          |                         ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:1
          |
        4 | mixed_quotes_str = 'single' "double" 'single'
          | ^^^^^^^^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:4:20
          |
        4 | mixed_quotes_str = 'single' "double" 'single'
          |                    ^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:4:29
          |
        4 | mixed_quotes_str = 'single' "double" 'single'
          |                             ^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:4:38
          |
        4 | mixed_quotes_str = 'single' "double" 'single'
          |                                      ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:1
          |
        5 | mixed_quotes_bytes = b'single' b"double" b'single'
          | ^^^^^^^^^^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:5:22
          |
        5 | mixed_quotes_bytes = b'single' b"double" b'single'
          |                      ^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:5:32
          |
        5 | mixed_quotes_bytes = b'single' b"double" b'single'
          |                                ^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:5:42
          |
        5 | mixed_quotes_bytes = b'single' b"double" b'single'
          |                                          ^^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:6:1
          |
        6 | regular_string = "just a string"
          | ^^^^^^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:6:18
          |
        6 | regular_string = "just a string"
          |                  ^^^^^^^^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:7:1
          |
        7 | regular_bytes = b"just bytes"
          | ^^^^^^^^^^^^^
          |

        info[semantic-token]: string
         --> main.py:7:17
          |
        7 | regular_bytes = b"just bytes"
          |                 ^^^^^^^^^^^^^
          |
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

        assert_snapshot!(test.semantic_tokens(None), @r#"
        info[semantic-token]: variable
         --> main.py:3:1
          |
        3 | name = "Alice"
          | ^^^^
          |

        info[semantic-token]: string
         --> main.py:3:8
          |
        3 | name = "Alice"
          |        ^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:4:1
          |
        4 | data = b"hello"
          | ^^^^
          |

        info[semantic-token]: string
         --> main.py:4:8
          |
        4 | data = b"hello"
          |        ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:5:1
          |
        5 | value = 42
          | ^^^^^
          |

        info[semantic-token]: number
         --> main.py:5:9
          |
        5 | value = 42
          |         ^^
          |

        info[semantic-token]: variable
         --> main.py:8:1
          |
        8 | result = f"Hello {name}! Value: {value}, Data: {data!r}"
          | ^^^^^^
          |

        info[semantic-token]: string
         --> main.py:8:12
          |
        8 | result = f"Hello {name}! Value: {value}, Data: {data!r}"
          |            ^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:8:19
          |
        8 | result = f"Hello {name}! Value: {value}, Data: {data!r}"
          |                   ^^^^
          |

        info[semantic-token]: string
         --> main.py:8:24
          |
        8 | result = f"Hello {name}! Value: {value}, Data: {data!r}"
          |                        ^^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:8:34
          |
        8 | result = f"Hello {name}! Value: {value}, Data: {data!r}"
          |                                  ^^^^^
          |

        info[semantic-token]: string
         --> main.py:8:40
          |
        8 | result = f"Hello {name}! Value: {value}, Data: {data!r}"
          |                                        ^^^^^^^^
          |

        info[semantic-token]: variable
         --> main.py:8:49
          |
        8 | result = f"Hello {name}! Value: {value}, Data: {data!r}"
          |                                                 ^^^^
          |

        info[semantic-token]: variable
          --> main.py:11:1
           |
        11 | mixed = f"prefix" + b"suffix"
           | ^^^^^
           |

        info[semantic-token]: string
          --> main.py:11:11
           |
        11 | mixed = f"prefix" + b"suffix"
           |           ^^^^^^
           |

        info[semantic-token]: string
          --> main.py:11:21
           |
        11 | mixed = f"prefix" + b"suffix"
           |                     ^^^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:14:1
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           | ^^^^^^^^^^^^^^^
           |

        info[semantic-token]: string
          --> main.py:14:21
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                     ^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:14:28
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                            ^^^^
           |

        info[semantic-token]: method
          --> main.py:14:33
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                                 ^^^^^
           |

        info[semantic-token]: string
          --> main.py:14:41
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                                         ^^^^^^^^^
           |

        info[semantic-token]: function
          --> main.py:14:51
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                                                   ^^^
           |

        info[semantic-token]: variable
          --> main.py:14:55
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                                                       ^^^^
           |

        info[semantic-token]: string
          --> main.py:14:61
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                                                             ^^^^^^^
           |

        info[semantic-token]: variable
          --> main.py:14:69
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                                                                     ^^^^^
           |

        info[semantic-token]: string
          --> main.py:14:75
           |
        14 | complex_fstring = f"User: {name.upper()}, Count: {len(data)}, Hex: {value:x}"
           |                                                                           ^
           |
        "#);
    }

    impl CursorTest {
        fn semantic_tokens(&self, range: Option<TextRange>) -> String {
            let tokens = semantic_tokens(&self.db, self.cursor.file, range);

            if tokens.is_empty() {
                return "No semantic found".to_string();
            }

            let config = DisplayDiagnosticConfig::default()
                .color(false)
                .format(DiagnosticFormat::Full)
                .context(0);

            self.render_diagnostics_with_config(
                tokens
                    .iter()
                    .map(|token| SemanticTokenDiagnostic::new(self.cursor.file, token)),
                &config,
            )
        }
    }

    struct SemanticTokenDiagnostic {
        source: FileRange,
        token_type: SemanticTokenType,
        modifiers: SemanticTokenModifier,
    }

    impl SemanticTokenDiagnostic {
        fn new(file: File, token: &SemanticToken) -> Self {
            Self {
                source: FileRange::new(file, token.range),
                token_type: token.token_type,
                modifiers: token.modifiers,
            }
        }
    }

    impl IntoDiagnostic for SemanticTokenDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("semantic-token")),
                Severity::Info,
                self.token_type.as_lsp_concept(),
            );

            let mut annotation =
                Annotation::primary(Span::from(self.source.file()).with_range(self.source.range()));

            let mut modifiers = String::new();

            for (modifier, _) in self.modifiers.iter_names() {
                if !modifiers.is_empty() {
                    modifiers.push_str(", ");
                }

                modifiers.push_str(modifier);
            }

            if !modifiers.is_empty() {
                annotation.set_message(modifiers);
            }

            main.annotate(annotation);

            main
        }
    }
}
