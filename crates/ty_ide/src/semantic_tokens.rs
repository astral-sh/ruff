use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, Stmt, TypeParam, TypeParams};
use ruff_text_size::{Ranged, TextLen, TextRange};
use std::ops::Deref;
use ty_python_semantic::{HasType, SemanticModel, types::Type};

// This module walks the AST and collects a set of "semantic tokens" for a file
// or a range within a file. Each semantic token provides a "token type" and zero
// or more "modifiers". This information can be used by an editor to provide
// color coding based on semantic meaning.

// Current limitations and areas for future improvement:

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
#[repr(u32)]
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
    /// Returns all supported token types for LSP capabilities.
    pub const fn all() -> [&'static str; 15] {
        [
            "namespace",
            "class",
            "parameter",
            "selfParameter",
            "clsParameter",
            "variable",
            "property",
            "function",
            "method",
            "keyword",
            "string",
            "number",
            "decorator",
            "builtinConstant",
            "typeParameter",
        ]
    }
}

/// Semantic token modifiers using bit flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticTokenModifier(u32);

impl SemanticTokenModifier {
    pub const DEFINITION: Self = Self(1 << 0);
    pub const READONLY: Self = Self(1 << 1);
    pub const ASYNC: Self = Self(1 << 2);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns all supported token modifiers for LSP capabilities.
    pub fn all() -> Vec<&'static str> {
        vec!["definition", "readonly", "async"]
    }

    /// Convert to LSP modifier indices for encoding
    pub fn to_lsp_indices(self) -> Vec<u32> {
        let mut indices = Vec::new();
        if self.contains(Self::DEFINITION) {
            indices.push(0);
        }
        if self.contains(Self::READONLY) {
            indices.push(1);
        }
        if self.contains(Self::ASYNC) {
            indices.push(2);
        }
        indices
    }
}

/// A semantic token with its position and classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub range: TextRange,
    pub token_type: SemanticTokenType,
    pub modifiers: SemanticTokenModifier,
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

    let mut visitor = SemanticTokenVisitor::new(db, &semantic_model, range);
    visitor.visit_body(parsed.suite());

    SemanticTokens::new(visitor.tokens)
}

/// AST visitor that collects semantic tokens.
struct SemanticTokenVisitor<'db> {
    #[allow(dead_code)]
    db: &'db dyn Db,
    #[allow(dead_code)]
    semantic_model: &'db SemanticModel<'db>,
    tokens: Vec<SemanticToken>,
    in_class_scope: bool,
    in_type_annotation: bool,
    range_filter: Option<TextRange>,
}

impl<'db> SemanticTokenVisitor<'db> {
    fn new(
        db: &'db dyn Db,
        semantic_model: &'db SemanticModel<'db>,
        range_filter: Option<TextRange>,
    ) -> Self {
        Self {
            db,
            semantic_model,
            tokens: Vec::new(),
            in_class_scope: false,
            in_type_annotation: false,
            range_filter,
        }
    }

    fn add_token(
        &mut self,
        range: TextRange,
        token_type: SemanticTokenType,
        modifiers: SemanticTokenModifier,
    ) {
        // Only emit tokens that intersect with the range filter, if one is specified
        if let Some(range_filter) = self.range_filter {
            if range.intersect(range_filter).is_none() {
                return;
            }
        }

        // Debug assertion to ensure tokens are added in file order
        debug_assert!(
            self.tokens.is_empty() || self.tokens.last().unwrap().range.start() <= range.start(),
            "Tokens must be added in file order: previous token ends at {:?}, new token starts at {:?}",
            self.tokens.last().map(|t| t.range.start()),
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
        // Try to get the inferred type of this name expression using semantic analysis
        let ty = name.inferred_type(self.semantic_model);
        let name_str = name.id.as_str();
        self.classify_from_type_and_name_str(ty, name_str)
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
                    modifiers = modifiers.union(SemanticTokenModifier::READONLY);
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
                    modifiers = modifiers.union(SemanticTokenModifier::READONLY);
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

    fn visit_type_params(&mut self, type_params: &TypeParams) {
        for type_param in &type_params.type_params {
            self.visit_type_param(type_param);
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

    /// Visit decorators, handling simple name decorators vs complex expressions
    fn visit_decorators(&mut self, decorators: &[ast::Decorator]) {
        for decorator in decorators {
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
    }

    fn visit_parameters(
        &mut self,
        parameters: &ast::Parameters,
        func: Option<&ast::StmtFunctionDef>,
    ) {
        // Parameters
        for (i, param) in parameters.args.iter().enumerate() {
            let token_type = if let Some(func) = func {
                // For function definitions, use the existing classification logic
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

    fn visit_body(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
        }
    }
}

impl SourceOrderVisitor<'_> for SemanticTokenVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        // If we have a range filter and this statement doesn't intersect, skip it
        // as an optimization
        if let Some(range_filter) = self.range_filter {
            if stmt.range().intersect(range_filter).is_none() {
                return;
            }
        }

        match stmt {
            ast::Stmt::FunctionDef(func) => {
                // Visit decorator expressions
                self.visit_decorators(&func.decorator_list);

                // Function name
                self.add_token(
                    func.name.range(),
                    if self.in_class_scope {
                        SemanticTokenType::Method
                    } else {
                        SemanticTokenType::Function
                    },
                    if func.is_async {
                        SemanticTokenModifier::DEFINITION.union(SemanticTokenModifier::ASYNC)
                    } else {
                        SemanticTokenModifier::DEFINITION
                    },
                );

                // Type parameters (Python 3.12+ syntax)
                if let Some(type_params) = &func.type_params {
                    self.visit_type_params(type_params);
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
                self.visit_decorators(&class.decorator_list);

                // Class name
                self.add_token(
                    class.name.range(),
                    SemanticTokenType::Class,
                    SemanticTokenModifier::DEFINITION,
                );

                // Type parameters (Python 3.12+ syntax)
                if let Some(type_params) = &class.type_params {
                    self.visit_type_params(type_params);
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
                    self.add_token(name.range(), token_type, modifiers);
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
                        self.add_token(asname.range(), token_type, modifiers);
                    } else {
                        // For direct imports (from X import Y), use semantic classification
                        let ty = alias.inferred_type(self.semantic_model);
                        let (token_type, modifiers) =
                            self.classify_from_alias_type(ty, &alias.name);
                        self.add_token(alias.name.range(), token_type, modifiers);
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
        // If we have a range filter and this statement doesn't intersect, skip it
        // as an optimization
        if let Some(range_filter) = self.range_filter {
            if expr.range().intersect(range_filter).is_none() {
                return;
            }
        }

        match expr {
            ast::Expr::Name(name) => {
                let (token_type, modifiers) = self.classify_name(name);
                self.add_token(name.range(), token_type, modifiers);
                walk_expr(self, expr);
            }
            ast::Expr::Attribute(attr) => {
                // Visit the base expression first (e.g., 'os' in 'os.path')
                self.visit_expr(&attr.value);

                // Then add token for the attribute name (e.g., 'path' in 'os.path')
                let ty = expr.inferred_type(self.semantic_model);
                let (token_type, modifiers) =
                    Self::classify_from_type_for_attribute(ty, &attr.attr);
                self.add_token(attr.attr.range(), token_type, modifiers);
            }
            ast::Expr::Call(call) => {
                // Visit the function being called first
                self.visit_expr(&call.func);

                // Visit arguments
                for arg in &call.arguments.args {
                    self.visit_expr(arg);
                }

                // Visit keyword arguments
                for keyword in &call.arguments.keywords {
                    self.visit_expr(&keyword.value);
                }
            }
            ast::Expr::StringLiteral(string_literal) => {
                // For implicitly concatenated strings, emit separate tokens for each string part
                for string_part in &string_literal.value {
                    self.add_token(
                        string_part.range(),
                        SemanticTokenType::String,
                        SemanticTokenModifier::empty(),
                    );
                }
                walk_expr(self, expr);
            }
            ast::Expr::BytesLiteral(bytes_literal) => {
                // For implicitly concatenated bytes, emit separate tokens for each bytes part
                for bytes_part in &bytes_literal.value {
                    self.add_token(
                        bytes_part.range(),
                        SemanticTokenType::String,
                        SemanticTokenModifier::empty(),
                    );
                }
                walk_expr(self, expr);
            }
            ast::Expr::NumberLiteral(_) => {
                self.add_token(
                    expr.range(),
                    SemanticTokenType::Number,
                    SemanticTokenModifier::empty(),
                );
                walk_expr(self, expr);
            }
            ast::Expr::BooleanLiteral(_) => {
                self.add_token(
                    expr.range(),
                    SemanticTokenType::BuiltinConstant,
                    SemanticTokenModifier::empty(),
                );
                walk_expr(self, expr);
            }
            ast::Expr::NoneLiteral(_) => {
                self.add_token(
                    expr.range(),
                    SemanticTokenType::BuiltinConstant,
                    SemanticTokenModifier::empty(),
                );
                walk_expr(self, expr);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::cursor_test;

    /// Helper function to assert exact token counts
    ///
    /// This function helps reduce boilerplate in tests by allowing you to specify
    /// the expected counts for multiple token types in a single call.
    ///
    /// # Example
    /// ```
    /// assert_token_counts(&tokens, &[
    ///     (SemanticTokenType::BuiltinConstant, 3),
    ///     (SemanticTokenType::Variable, 3),
    /// ]);
    /// ```
    fn assert_token_counts(
        tokens: &SemanticTokens,
        expected_counts: &[(SemanticTokenType, usize)],
    ) {
        for (token_type, expected_count) in expected_counts {
            let actual_count = tokens
                .iter()
                .filter(|t| t.token_type == *token_type)
                .count();
            assert_eq!(
                actual_count, *expected_count,
                "Expected {expected_count} tokens of type {token_type:?}, but found {actual_count}"
            );
        }
    }

    /// Helper function to get semantic tokens for full file (for testing)
    fn semantic_tokens_full_file(db: &dyn Db, file: File) -> SemanticTokens {
        semantic_tokens(db, file, None)
    }

    #[test]
    fn test_semantic_tokens_basic() {
        let test = cursor_test("def foo(): pass<CURSOR>");

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);
        assert!(!tokens.is_empty());

        assert_token_counts(&tokens, &[(SemanticTokenType::Function, 1)]);
    }

    #[test]
    fn test_semantic_tokens_class() {
        let test = cursor_test("class MyClass: pass<CURSOR>");

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        assert_token_counts(&tokens, &[(SemanticTokenType::Class, 1)]);
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

        assert_token_counts(
            &tokens,
            &[
                (SemanticTokenType::Variable, 2),
                (SemanticTokenType::Number, 1),
                (SemanticTokenType::String, 1),
            ],
        );
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

        assert_token_counts(
            &tokens,
            &[
                (SemanticTokenType::SelfParameter, 1),
                (SemanticTokenType::Parameter, 1),
            ],
        );
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

        // Should have a cls parameter token
        let cls_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::ClsParameter))
            .collect();
        assert!(!cls_tokens.is_empty());

        // Should have a regular parameter token for x
        let param_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Parameter))
            .collect();
        assert!(!param_tokens.is_empty());
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

        // Should have only regular parameter tokens (no self/cls)
        let param_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Parameter))
            .collect();
        assert_eq!(
            param_tokens.len(),
            2,
            "Expected exactly 2 parameter tokens (x, y)"
        );

        // Should not have self or cls parameter tokens
        let self_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::SelfParameter))
            .collect();
        assert!(self_tokens.is_empty());

        let cls_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::ClsParameter))
            .collect();
        assert!(cls_tokens.is_empty());
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

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        // Should have a self parameter token for "instance"
        let self_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::SelfParameter))
            .collect();
        assert!(!self_tokens.is_empty());

        // Should have a cls parameter token for "klass"
        let cls_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::ClsParameter))
            .collect();
        assert!(!cls_tokens.is_empty());

        // Should have regular parameter tokens for x and y
        let param_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Parameter))
            .collect();
        assert_eq!(
            param_tokens.len(),
            2,
            "Expected exactly 2 parameter tokens (x, y)"
        );
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

        // Should have a class token with Definition modifier
        let class_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Class))
            .collect();
        assert!(!class_tokens.is_empty());
        assert!(
            class_tokens[0]
                .modifiers
                .contains(SemanticTokenModifier::DEFINITION)
        );

        // Should have a constant with Readonly modifier
        let constant_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                matches!(
                    t.token_type,
                    SemanticTokenType::Property | SemanticTokenType::Variable
                ) && t.modifiers.contains(SemanticTokenModifier::READONLY)
            })
            .collect();
        assert!(!constant_tokens.is_empty());

        // Should have an async method with Async modifier
        let async_method_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                matches!(t.token_type, SemanticTokenType::Method)
                    && t.modifiers.contains(SemanticTokenModifier::ASYNC)
            })
            .collect();
        assert!(!async_method_tokens.is_empty());
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

        // Should have module tokens for imports
        let module_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Namespace))
            .collect();
        assert!(!module_tokens.is_empty());

        // Should have class tokens
        let class_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Class))
            .collect();
        assert!(!class_tokens.is_empty());

        // Should have function tokens
        let function_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Function))
            .collect();
        assert!(!function_tokens.is_empty());

        // Should have variable tokens for assignments
        let variable_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Variable))
            .collect();
        assert_eq!(variable_tokens.len(), 4); // x, y, z, version (from sys.version)
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

        assert_token_counts(
            &tokens,
            &[
                (SemanticTokenType::BuiltinConstant, 3),
                (SemanticTokenType::Variable, 3),
            ],
        );
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

        assert_token_counts(
            &tokens,
            &[
                (SemanticTokenType::BuiltinConstant, 4), // None, False, True, None
                (SemanticTokenType::Function, 2),        // check, check (in call)
            ],
        );
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

        // Should still have tokens for function2, y, z, "hello", True
        let function_tokens: Vec<_> = range_tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Function))
            .collect();
        assert!(!function_tokens.is_empty()); // function2

        let variable_tokens: Vec<_> = range_tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Variable))
            .collect();
        assert_eq!(
            variable_tokens.len(),
            4,
            "Expected exactly 4 variable tokens (y, z, y, z)"
        );

        let string_tokens: Vec<_> = range_tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::String))
            .collect();
        assert!(!string_tokens.is_empty()); // "hello"

        let builtin_tokens: Vec<_> = range_tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::BuiltinConstant))
            .collect();
        assert!(!builtin_tokens.is_empty()); // True

        // Verify that no tokens from range_tokens have ranges outside the requested range
        for token in range_tokens.iter() {
            assert!(
                range.contains_range(token.range),
                "Token at {:?} is outside requested range {:?}",
                token.range,
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

        // Should have module tokens for each part of dotted names
        let module_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Namespace))
            .collect();

        // Should have tokens for: os, path, sys, version_info, urllib, parse, collections, abc
        // That's 8 separate module tokens
        assert_eq!(
            module_tokens.len(),
            8,
            "Expected exactly 8 module tokens, got {}",
            module_tokens.len()
        );

        // Should have tokens for imported names with correct classifications
        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // urlparse should be classified based on its actual semantic type (likely Function)
        let urlparse_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "urlparse"
            })
            .collect();
        assert_eq!(urlparse_tokens.len(), 1, "Expected 1 token for urlparse");
        // urlparse is a function, so it should be classified as Function
        assert!(
            matches!(
                urlparse_tokens[0].token_type,
                SemanticTokenType::Function | SemanticTokenType::Variable
            ),
            "urlparse should be classified as Function or Variable"
        );

        // Mapping should be classified as a class (ABC/Protocol)
        let mapping_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "Mapping" && matches!(t.token_type, SemanticTokenType::Class)
            })
            .collect();
        assert_eq!(
            mapping_tokens.len(),
            1,
            "Expected 1 class token for Mapping"
        );
        let mapping_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "Mapping" && matches!(t.token_type, SemanticTokenType::Class)
            })
            .collect();
        assert_eq!(
            mapping_tokens.len(),
            1,
            "Expected 1 class token for Mapping"
        );

        // Verify that none of the module tokens contain periods
        // by checking that each token's text length matches what we expect
        let source = ruff_db::source::source_text(&test.db, test.cursor.file);
        for token in &module_tokens {
            let token_text = &source[token.range];
            assert!(
                !token_text.contains('.'),
                "Module token should not contain periods: '{token_text}'"
            );
        }
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

        // Find tokens for imported modules
        let source = ruff_db::source::source_text(&test.db, test.cursor.file);
        let module_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                matches!(t.token_type, SemanticTokenType::Namespace)
                    && (token_text == "os" || token_text == "sys")
            })
            .collect();

        // Should have 4 namespace tokens: os, sys (in imports), os, sys (in assignments)
        assert_eq!(
            module_tokens.len(),
            4,
            "Expected 4 namespace tokens for module references, got {}",
            module_tokens.len()
        );

        // Verify that variables assigned to modules are also classified as namespace
        let xy_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                (token_text == "x" || token_text == "y")
                    && matches!(t.token_type, SemanticTokenType::Namespace)
            })
            .collect();

        // Should have 2 namespace tokens for x and y (since they hold module values)
        assert_eq!(
            xy_tokens.len(),
            2,
            "Expected 2 namespace tokens for x and y, got {}",
            xy_tokens.len()
        );

        // Verify that defaultdict is classified based on its semantic type (likely Class since it's a constructor)
        let defaultdict_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "defaultdict"
            })
            .collect();

        assert_eq!(
            defaultdict_tokens.len(),
            1,
            "Expected 1 token for defaultdict, got {}",
            defaultdict_tokens.len()
        );
        // defaultdict is actually a class constructor, so it should be classified as Class
        assert!(
            matches!(
                defaultdict_tokens[0].token_type,
                SemanticTokenType::Class | SemanticTokenType::Variable
            ),
            "defaultdict should be classified as Class or Variable based on semantic analysis"
        );
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // path should be classified as namespace (since os.path is actually a module)
        let path_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "path" && matches!(t.token_type, SemanticTokenType::Namespace)
            })
            .collect();
        assert_eq!(
            path_tokens.len(),
            1,
            "Expected 1 namespace token for path (os.path is a module)"
        );

        // defaultdict should be classified based on its actual semantic type (likely Function)
        let defaultdict_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "defaultdict"
            })
            .collect();

        if defaultdict_tokens.is_empty() {
            panic!("No tokens found for 'defaultdict'");
        } else {
            // defaultdict is actually a class constructor, so it might be classified as Class
            let token_type = &defaultdict_tokens[0].token_type;
            assert!(
                matches!(
                    token_type,
                    SemanticTokenType::Variable
                        | SemanticTokenType::Class
                        | SemanticTokenType::Function
                ),
                "defaultdict should be classified as Variable, Class, or Function, got {token_type:?}"
            );
        }

        // The remaining tests are more flexible since semantic analysis might not have complete info
        // for all imports, especially from unresolved modules

        // Just verify that we have tokens for the expected imported names
        let expected_names = vec![
            "OrderedDict",
            "Counter",
            "List",
            "Dict",
            "Optional",
            "CONSTANT",
            "my_function",
            "MyClass",
        ];
        for name in expected_names {
            let name_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| {
                    let token_text = &source[t.range];
                    token_text == name
                })
                .collect();
            assert!(
                !name_tokens.is_empty(),
                "Expected at least 1 token for {name}"
            );
        }

        // CONSTANT should have readonly modifier if it's classified as Variable
        let constant_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "CONSTANT"
            })
            .collect();
        if !constant_tokens.is_empty()
            && matches!(constant_tokens[0].token_type, SemanticTokenType::Variable)
        {
            assert!(
                constant_tokens[0]
                    .modifiers
                    .contains(SemanticTokenModifier::READONLY),
                "CONSTANT should have readonly modifier when classified as Variable"
            );
        }
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Find all tokens and create a map for easier testing
        let mut token_map = std::collections::HashMap::new();
        for token in tokens.iter() {
            let token_text = &source[token.range];
            token_map
                .entry(token_text.to_string())
                .or_insert_with(Vec::new)
                .push(token);
        }

        // Test path attribute (should be namespace since os.path is a module)
        let path_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "path" && matches!(t.token_type, SemanticTokenType::Namespace)
            })
            .collect();
        assert!(
            !path_tokens.is_empty(),
            "Expected at least 1 namespace token for 'path' attribute"
        );

        // Test method attribute (should be method - bound method)
        let method_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "method" && matches!(t.token_type, SemanticTokenType::Method)
            })
            .collect();
        assert!(
            !method_tokens.is_empty(),
            "Expected at least 1 method token for 'method' attribute"
        );

        // Test CONSTANT attribute (should be variable with readonly modifier)
        let constant_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "CONSTANT"
                    && matches!(
                        t.token_type,
                        SemanticTokenType::Variable | SemanticTokenType::Property
                    )
                    && t.modifiers.contains(SemanticTokenModifier::READONLY)
            })
            .collect();
        assert!(
            !constant_tokens.is_empty(),
            "Expected at least 1 variable/property token with readonly modifier for 'CONSTANT' attribute"
        );

        // Test property attribute (should be property)
        // Note: This might not work perfectly if the semantic analyzer doesn't have full property info
        // but we should have at least a variable token for it
        let prop_or_var_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "prop"
                    && matches!(
                        t.token_type,
                        SemanticTokenType::Property | SemanticTokenType::Variable
                    )
            })
            .collect();
        assert!(
            !prop_or_var_tokens.is_empty(),
            "Expected at least 1 property/variable token for 'prop' attribute"
        );

        // Test __name__ attribute (should be variable since it's a string attribute)
        let name_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "__name__" && matches!(t.token_type, SemanticTokenType::Variable)
            })
            .collect();
        assert!(
            !name_tokens.is_empty(),
            "Expected at least 1 variable token for '__name__' attribute"
        );
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Test that attributes with unknown/basic types fall back to variable, not property
        let attr_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                (token_text == "some_attr" || token_text == "unknown_attr")
                    && matches!(
                        t.token_type,
                        SemanticTokenType::Variable | SemanticTokenType::Property
                    )
            })
            .collect();

        // We should have tokens for both attributes plus the class definition
        // some_attr appears twice (class definition + attribute access) + unknown_attr (attribute access)
        assert_eq!(
            attr_tokens.len(),
            3,
            "Expected exactly 3 tokens for attribute expressions: some_attr (definition), some_attr (access), unknown_attr (access)"
        );

        // With our new implementation, the fallback should be Variable, not Property
        // However, since semantic analysis might classify some as Property based on context,
        // we'll be flexible here but ensure we have the expected behavior
        assert!(
            !attr_tokens.is_empty(),
            "Expected attribute tokens to be classified"
        );
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Test UPPER_CASE (should have readonly modifier)
        let upper_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "UPPER_CASE" && t.modifiers.contains(SemanticTokenModifier::READONLY)
            })
            .collect();
        assert!(
            !upper_tokens.is_empty(),
            "Expected UPPER_CASE to have readonly modifier"
        );

        // Test lower_case (should not have readonly modifier)
        let lower_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "lower_case" && !t.modifiers.contains(SemanticTokenModifier::READONLY)
            })
            .collect();
        assert!(
            !lower_tokens.is_empty(),
            "Expected lower_case to not have readonly modifier"
        );

        // Test MixedCase (should not have readonly modifier)
        let mixed_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "MixedCase" && !t.modifiers.contains(SemanticTokenModifier::READONLY)
            })
            .collect();
        assert!(
            !mixed_tokens.is_empty(),
            "Expected MixedCase to not have readonly modifier"
        );

        // Test A (should not have readonly modifier - single character)
        let a_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "A" && !t.modifiers.contains(SemanticTokenModifier::READONLY)
            })
            .collect();
        assert!(
            !a_tokens.is_empty(),
            "Expected A to not have readonly modifier (single character)"
        );
    }

    #[test]
    fn test_type_annotations() {
        let test = cursor_test(
            r#"
from typing import List, Dict, Optional, Union
from collections import defaultdict

class MyClass:
    pass

def function_with_annotations(
    param1: int,
    param2: str, 
    param3: List[str],
    param4: Dict[str, int],
    param5: Optional[MyClass],
    param6: Union[int, str],
    param7: defaultdict[str, int]
) -> Optional[List[MyClass]]:
    pass

# Variable type annotations
x: int = 42
y: str = "hello"
z: List[int] = [1, 2, 3]
w: MyClass = MyClass()
v: Optional[str] = None

# Class with type annotations
class TypedClass(List[str]):
    attr1: int
    attr2: Dict[str, MyClass]
    
    def __init__(self, value: str) -> None:
        self.attr1 = 0<CURSOR>
"#,
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Test basic type annotations (int, str should be classified as class/variable)
        let int_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "int"
            })
            .collect();
        assert!(
            !int_tokens.is_empty(),
            "Expected tokens for 'int' type annotations"
        );

        let str_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "str"
            })
            .collect();
        assert!(
            !str_tokens.is_empty(),
            "Expected tokens for 'str' type annotations"
        );

        // Test generic type annotations (List, Dict, Optional, Union should be classified based on semantic info)
        let list_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "List"
            })
            .collect();
        assert!(
            !list_tokens.is_empty(),
            "Expected tokens for 'List' type annotations"
        );

        let dict_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "Dict"
            })
            .collect();
        assert!(
            !dict_tokens.is_empty(),
            "Expected tokens for 'Dict' type annotations"
        );

        let optional_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "Optional"
            })
            .collect();
        assert!(
            !optional_tokens.is_empty(),
            "Expected tokens for 'Optional' type annotations"
        );

        let union_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "Union"
            })
            .collect();
        assert!(
            !union_tokens.is_empty(),
            "Expected tokens for 'Union' type annotations"
        );

        // Test custom class in type annotations (MyClass should be classified as class)
        let myclass_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "MyClass" && matches!(t.token_type, SemanticTokenType::Class)
            })
            .collect();
        assert!(
            !myclass_tokens.is_empty(),
            "Expected 'MyClass' in type annotations to be classified as Class"
        );

        // Test imported types in annotations (defaultdict should be classified based on semantic info)
        let defaultdict_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "defaultdict"
            })
            .collect();
        assert!(
            !defaultdict_tokens.is_empty(),
            "Expected tokens for 'defaultdict' type annotations"
        );

        // Verify parameters are still classified correctly
        let param_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Parameter))
            .collect();
        assert!(
            param_tokens.len() >= 7,
            "Expected at least 7 parameter tokens"
        );

        // Verify function names are still classified correctly
        let function_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Function))
            .collect();
        assert!(!function_tokens.is_empty(), "Expected function tokens");

        // Verify variable names in annotated assignments are classified correctly
        let variable_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                (token_text == "x"
                    || token_text == "y"
                    || token_text == "z"
                    || token_text == "w"
                    || token_text == "v")
                    && matches!(t.token_type, SemanticTokenType::Variable)
            })
            .collect();
        assert!(
            variable_tokens.len() >= 5,
            "Expected at least 5 variable tokens for annotated assignments"
        );

        // Test class inheritance with type annotations (MyClass should be classified as class)
        // The List in the class bases should also be properly classified
        let inheritance_list_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "List" // All List tokens, including in inheritance
            })
            .collect();
        assert!(
            inheritance_list_tokens.len() >= 2,
            "Expected at least 2 'List' tokens (in parameters and inheritance)"
        );
    }

    #[test]
    fn test_debug_int_classification() {
        let test = cursor_test(
            "
x: int = 42<CURSOR>
",
        );

        let tokens = semantic_tokens(&test.db, test.cursor.file, None);

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Find int tokens specifically
        let int_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "int"
            })
            .collect();

        // Verify int in type annotation is classified as Class
        assert_eq!(int_tokens.len(), 1, "Expected exactly 1 int token");
        assert!(
            matches!(int_tokens[0].token_type, SemanticTokenType::Class),
            "int in type annotation should be classified as Class"
        );

        // Verify variable x is classified as Variable
        let x_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "x" && matches!(t.token_type, SemanticTokenType::Variable)
            })
            .collect();
        assert_eq!(x_tokens.len(), 1, "Expected exactly 1 variable token for x");
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Find MyClass tokens specifically
        let myclass_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "MyClass"
            })
            .collect();

        // Should have multiple MyClass tokens:
        // 1. Class definition (Class with Definition modifier)
        // 2. Type annotation (Class)
        // 3. Constructor call (should be Class, might be duplicated)
        assert!(
            myclass_tokens.len() >= 3,
            "Expected at least 3 MyClass tokens"
        );

        // Verify class definition token
        let def_tokens: Vec<_> = myclass_tokens
            .iter()
            .filter(|t| t.modifiers.contains(SemanticTokenModifier::DEFINITION))
            .collect();
        assert_eq!(
            def_tokens.len(),
            1,
            "Expected exactly 1 MyClass definition token"
        );
        assert!(matches!(def_tokens[0].token_type, SemanticTokenType::Class));

        // Verify type annotation token
        let annotation_tokens: Vec<_> = myclass_tokens
            .iter()
            .filter(|t| {
                matches!(t.token_type, SemanticTokenType::Class)
                    && !t.modifiers.contains(SemanticTokenModifier::DEFINITION)
            })
            .collect();
        assert!(
            !annotation_tokens.is_empty(),
            "Expected at least 1 MyClass type annotation token"
        );

        // Verify variable x is classified as Variable
        let x_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "x" && matches!(t.token_type, SemanticTokenType::Variable)
            })
            .collect();
        assert_eq!(x_tokens.len(), 1, "Expected exactly 1 variable token for x");
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Check that variable assignment targets are Variable tokens
        let variable_names = ["x", "y", "z"];
        for var_name in variable_names {
            let var_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| {
                    let token_text = &source[t.range];
                    token_text == var_name && matches!(t.token_type, SemanticTokenType::Variable)
                })
                .collect();
            assert!(
                !var_tokens.is_empty(),
                "Expected variable token for {var_name}"
            );
        }

        // Check that basic type names in annotations are Class tokens
        let basic_type_names = ["int", "str"];
        for type_name in basic_type_names {
            let type_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| {
                    let token_text = &source[t.range];
                    token_text == type_name && matches!(t.token_type, SemanticTokenType::Class)
                })
                .collect();
            assert!(
                !type_tokens.is_empty(),
                "Expected class token for {type_name} in type annotations"
            );
        }

        // Check that user-defined class names in annotations are Class tokens
        let myclass_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "MyClass" && matches!(t.token_type, SemanticTokenType::Class)
            })
            .collect();
        assert!(
            !myclass_tokens.is_empty(),
            "Expected class token for MyClass in type annotations"
        );

        // Check that imported types exist (classification may vary based on semantic analysis)
        let imported_type_names = ["List", "Optional"];
        for type_name in imported_type_names {
            let type_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| {
                    let token_text = &source[t.range];
                    token_text == type_name
                })
                .collect();
            assert!(!type_tokens.is_empty(), "Expected tokens for {type_name}");
        }

        // Check that parameters are Parameter tokens
        let param_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                (token_text == "param" || token_text == "other")
                    && matches!(t.token_type, SemanticTokenType::Parameter)
            })
            .collect();
        assert_eq!(param_tokens.len(), 2, "Expected 2 parameter tokens");

        // Check that function name is Function token
        let func_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "test_function" && matches!(t.token_type, SemanticTokenType::Function)
            })
            .collect();
        assert_eq!(func_tokens.len(), 1, "Expected 1 function token");
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Check that MyProtocol in type annotation is classified as Class
        let protocol_in_annotation_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "MyProtocol" && matches!(t.token_type, SemanticTokenType::Class)
            })
            .collect();

        // We expect at least one MyProtocol token to be classified as Class
        // (the one in the type annotation)
        assert!(
            !protocol_in_annotation_tokens.is_empty(),
            "Expected MyProtocol in type annotation to be classified as Class token"
        );
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Count MyProtocol tokens classified as Class
        let class_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "MyProtocol" && matches!(t.token_type, SemanticTokenType::Class)
            })
            .collect();

        // Note: We don't currently handle regular assignment targets (only annotated assignments)
        // So we expect 3 MyProtocol tokens (definition + 2 type annotations), not 4
        assert!(
            class_tokens.len() >= 3,
            "Expected at least 3 MyProtocol tokens classified as Class, got {}",
            class_tokens.len()
        );

        // Verify that one has Definition modifier (the class definition)
        let definition_tokens: Vec<_> = class_tokens
            .iter()
            .filter(|t| t.modifiers.contains(SemanticTokenModifier::DEFINITION))
            .collect();
        assert_eq!(
            definition_tokens.len(),
            1,
            "Expected exactly 1 MyProtocol token with Definition modifier, got {}",
            definition_tokens.len()
        );
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Count type parameter tokens
        let type_param_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::TypeParameter))
            .collect();

        // We should have type parameter tokens for:
        // - T (declaration in func), T (in parameter type), T (in return type)
        // - Ts (declaration in func_tuple), Ts (in parameter type), Ts (in return type)
        // - P (declaration in func_paramspec), P (in parameter type), P (in return type), P.args, P.kwargs
        // - T, U (declarations in Container), T (in __init__ param), U (in __init__ param),
        //   T (in value1 annotation), T (in value2 annotation), T (in get_first return), U (in get_second return)
        // - T, U (declarations in BoundedContainer), T (in process param), U (in process param),
        //   T (in return tuple), U (in return tuple)

        // Let's be conservative and expect at least the declaration tokens plus some usage tokens
        assert!(
            type_param_tokens.len() >= 8,
            "Expected at least 8 type parameter tokens (declarations + some usages), got {}",
            type_param_tokens.len()
        );

        // Check that T declarations have Definition modifier
        let t_definition_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "T"
                    && matches!(t.token_type, SemanticTokenType::TypeParameter)
                    && t.modifiers.contains(SemanticTokenModifier::DEFINITION)
            })
            .collect();

        // Should have T definition tokens from func, Container, and BoundedContainer
        assert!(
            t_definition_tokens.len() >= 3,
            "Expected at least 3 T definition tokens, got {}",
            t_definition_tokens.len()
        );

        // Check that U declarations have Definition modifier
        let u_definition_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "U"
                    && matches!(t.token_type, SemanticTokenType::TypeParameter)
                    && t.modifiers.contains(SemanticTokenModifier::DEFINITION)
            })
            .collect();

        // Should have U definition tokens from Container and BoundedContainer
        assert!(
            u_definition_tokens.len() >= 2,
            "Expected at least 2 U definition tokens, got {}",
            u_definition_tokens.len()
        );

        // Check that type parameter usages (without Definition modifier) are also classified correctly
        let t_usage_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "T"
                    && matches!(t.token_type, SemanticTokenType::TypeParameter)
                    && !t.modifiers.contains(SemanticTokenModifier::DEFINITION)
            })
            .collect();

        // Should have T usage tokens in type annotations
        assert!(
            !t_usage_tokens.is_empty(),
            "Expected T usage tokens in type annotations"
        );
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

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Find all T tokens classified as TypeParameter
        let t_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "T" && matches!(t.token_type, SemanticTokenType::TypeParameter)
            })
            .collect();

        // We should have at least:
        // - 1 definition (in the function signature)
        // - 1 usage in parameter type annotation
        // - 1 usage in return type annotation
        // - 1 usage in the variable annotation inside the function
        assert!(
            t_tokens.len() >= 4,
            "Expected at least 4 T tokens classified as TypeParameter, got {}",
            t_tokens.len()
        );

        // Check that exactly one has Definition modifier (the declaration)
        let definition_tokens: Vec<_> = t_tokens
            .iter()
            .filter(|t| t.modifiers.contains(SemanticTokenModifier::DEFINITION))
            .collect();
        assert_eq!(
            definition_tokens.len(),
            1,
            "Expected exactly 1 T token with Definition modifier, got {}",
            definition_tokens.len()
        );

        // Check that the others don't have Definition modifier (they are usages)

        let usage_tokens: Vec<_> = t_tokens
            .iter()
            .filter(|t| !t.modifiers.contains(SemanticTokenModifier::DEFINITION))
            .collect();
        assert!(
            usage_tokens.len() >= 3,
            "Expected at least 3 T usage tokens, got {}",
            usage_tokens.len()
        );
    }

    #[test]
    fn test_decorator_classification() {
        let test = cursor_test(
            r#"
@staticmethod
@classmethod  
@property
def simple_decorators():
    pass

@app.route("/path")
@cache.memoize(timeout=300)
@functools.wraps(other_func)
def complex_decorators():
    pass

@dataclass
@some_module.decorator_func
class MyClass:
    pass<CURSOR>
"#,
        );

        let tokens = semantic_tokens_full_file(&test.db, test.cursor.file);

        let source = ruff_db::source::source_text(&test.db, test.cursor.file);

        // Simple decorators should be classified as Decorator tokens
        let simple_decorator_names = vec!["staticmethod", "classmethod", "property", "dataclass"];
        for name in simple_decorator_names {
            let decorator_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| {
                    let token_text = &source[t.range];
                    token_text == name && matches!(t.token_type, SemanticTokenType::Decorator)
                })
                .collect();
            assert!(
                !decorator_tokens.is_empty(),
                "Expected decorator token for '{name}'"
            );
        }

        // Complex decorators should use normal expression classification
        // For example, "app" in "@app.route" should be classified as Variable/Function, not Decorator
        let app_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "app" && !matches!(t.token_type, SemanticTokenType::Decorator)
            })
            .collect();
        assert!(
            !app_tokens.is_empty(),
            "Expected 'app' to not be classified as Decorator"
        );

        // "route" in "@app.route" should be classified as Method/Function, not Decorator
        let route_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "route" && !matches!(t.token_type, SemanticTokenType::Decorator)
            })
            .collect();
        assert!(
            !route_tokens.is_empty(),
            "Expected 'route' to not be classified as Decorator"
        );

        // "some_module" should be classified as Namespace/Variable, not Decorator
        let module_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                let token_text = &source[t.range];
                token_text == "some_module" && !matches!(t.token_type, SemanticTokenType::Decorator)
            })
            .collect();
        assert!(
            !module_tokens.is_empty(),
            "Expected 'some_module' to not be classified as Decorator"
        );

        // Verify that function and class names are still classified correctly
        let function_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Function))
            .collect();
        assert!(
            function_tokens.len() >= 2,
            "Expected at least 2 function tokens"
        );

        let class_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::Class))
            .collect();
        assert!(!class_tokens.is_empty(), "Expected at least 1 class token");
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

        assert_token_counts(
            &tokens,
            &[
                (SemanticTokenType::Variable, 3),
                (SemanticTokenType::String, 8),
            ],
        );

        // Verify that we get individual tokens for each string literal part
        let source = ruff_db::source::source_text(&test.db, test.cursor.file);
        let string_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::String))
            .collect();

        // We should have tokens for each individual string part:
        // "hello", "world", "multi", "line", "string", 'single', "mixed", 'quotes'
        let expected_strings = vec![
            "\"hello\"",
            "\"world\"",
            "\"multi\"",
            "\"line\"",
            "\"string\"",
            "'single'",
            "\"mixed\"",
            "'quotes'",
        ];

        for expected_str in expected_strings {
            let matching_tokens: Vec<_> = string_tokens
                .iter()
                .filter(|t| &source[t.range] == expected_str)
                .collect();
            assert_eq!(
                matching_tokens.len(),
                1,
                "Expected exactly 1 token for {expected_str}, got {}",
                matching_tokens.len()
            );
        }
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

        assert_token_counts(
            &tokens,
            &[
                (SemanticTokenType::Variable, 3),
                (SemanticTokenType::String, 8), // treating bytes as strings
            ],
        );

        // Verify that we get individual tokens for each bytes literal part
        let source = ruff_db::source::source_text(&test.db, test.cursor.file);
        let string_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::String))
            .collect();

        // We should have tokens for each individual bytes part:
        // b"hello", b"world", b"multi", b"line", b"bytes", b'single', b"mixed", b'quotes'
        let expected_bytes = vec![
            "b\"hello\"",
            "b\"world\"",
            "b\"multi\"",
            "b\"line\"",
            "b\"bytes\"",
            "b'single'",
            "b\"mixed\"",
            "b'quotes'",
        ];

        for expected_bytes_str in expected_bytes {
            let matching_tokens: Vec<_> = string_tokens
                .iter()
                .filter(|t| &source[t.range] == expected_bytes_str)
                .collect();
            assert_eq!(
                matching_tokens.len(),
                1,
                "Expected exactly 1 token for {expected_bytes_str}, got {}",
                matching_tokens.len()
            );
        }
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

        assert_token_counts(
            &tokens,
            &[
                (SemanticTokenType::Variable, 6),
                (SemanticTokenType::String, 12),
            ],
        );

        // Verify specific token ranges
        let source = ruff_db::source::source_text(&test.db, test.cursor.file);
        let string_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.token_type, SemanticTokenType::String))
            .collect();

        // Check that we have tokens for regular and concatenated string literals
        // Note: We don't check for overlapping patterns like 'single' since they appear multiple times
        let unique_expected_literals = vec![
            "\"hello\"",
            "\"world\"", // string concat
            "b\"hello\"",
            "b\"world\"",        // bytes concat
            "\"double\"",        // from mixed quotes string
            "b\"double\"",       // from mixed quotes bytes
            "\"just a string\"", // regular string
            "b\"just bytes\"",   // regular bytes
        ];

        for expected_literal in unique_expected_literals {
            let matching_tokens: Vec<_> = string_tokens
                .iter()
                .filter(|t| &source[t.range] == expected_literal)
                .collect();
            assert_eq!(
                matching_tokens.len(),
                1,
                "Expected exactly 1 token for {expected_literal}, got {}",
                matching_tokens.len()
            );
        }

        // Check that 'single' appears exactly 2 times (once in string, once in bytes)
        let single_quote_tokens: Vec<_> = string_tokens
            .iter()
            .filter(|t| &source[t.range] == "'single'")
            .collect();
        assert_eq!(single_quote_tokens.len(), 2);

        // Check that b'single' appears exactly 2 times
        let bytes_single_quote_tokens: Vec<_> = string_tokens
            .iter()
            .filter(|t| &source[t.range] == "b'single'")
            .collect();
        assert_eq!(bytes_single_quote_tokens.len(), 2);
    }
}
