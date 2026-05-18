use std::{fmt, vec};

use rustc_hash::FxHashMap;

use crate::importer::{ImportAction, ImportRequest, Importer, MembersInScope};
use crate::{Db, HasNavigationTargets, NavigationTarget};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, ArgOrKeyword, Expr, ExprUnaryOp, Stmt, UnaryOp};
use ruff_python_codegen::Stylist;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_module_resolver::file_to_module;
use ty_python_semantic::types::ide_support::inlay_hint_call_argument_details;
use ty_python_semantic::types::{Type, TypeDetail};
use ty_python_semantic::{HasType, SemanticModel};

#[derive(Debug, Clone)]
pub struct InlayHint {
    pub position: TextSize,
    pub kind: InlayHintKind,
    pub label: InlayHintLabel,
    pub text_edits: Vec<InlayHintTextEdit>,
}

impl InlayHint {
    fn variable_type(
        context: InlayHintImportContext,
        expr: &Expr,
        rhs: &Expr,
        ty: Type,
        mut allow_edits: bool,
    ) -> Option<Self> {
        let InlayHintImportContext {
            db,
            file,
            importer,
            dynamic_imports,
        } = context;

        let position = expr.range().end();
        // Render the type to a string, and get subspans for all the types that make it up
        let details = ty.display(db).to_string_parts();

        // Filter out repetitive hints like `x: T = T()`
        if call_matches_name(rhs, &details.label) {
            return None;
        }

        let mut dynamic_importer = DynamicImporter::new(importer, expr, dynamic_imports);

        // Ok so the idea here is that we potentially have a random soup of spans here,
        // and each byte of the string can have at most one target associate with it.
        // Thankfully, they were generally pushed in print order, with the inner smaller types
        // appearing before the outer bigger ones.
        //
        // So we record where we are in the string, and every time we find a type, we
        // check if it's further along in the string. If it is, great, we give it the
        // span for its range, and then advance where we are.
        let mut offset = 0;

        // This edit label could be different from the original label if we need to
        // qualify certain imported symbols. `A` could turn into `foo.A`.
        let mut edit_label = details.label.clone();
        let mut edit_offset: isize = 0;

        let mut label_parts = vec![": ".into()];
        for (target, detail) in details.targets.iter().zip(&details.details) {
            match detail {
                TypeDetail::Type(ty) => {
                    let start = target.start().to_usize();
                    let end = target.end().to_usize();
                    // If we skipped over some bytes, push them with no target
                    if start > offset {
                        label_parts.push(details.label[offset..start].into());
                    }

                    // Possibly import the current type and return the qualified name
                    let mut qualified_name = |dynamic_importer: &mut DynamicImporter| {
                        let type_definition = ty.definition(db)?;
                        let definition = type_definition.definition()?;

                        // Only module-level names can be imported with `from <module> import <name>`.
                        // If the definition lives in a class or function body we can't produce a safe edit.
                        if !definition.file_scope(db).is_global() {
                            allow_edits = false;
                            return None;
                        }

                        // Don't try to import symbols in scope
                        if definition.file(db) == file {
                            return None;
                        }

                        let definition_name = definition.name(db);

                        // Fallback to the label if we cannot find the name
                        let definition_name = definition_name
                            .as_deref()
                            .unwrap_or(&details.label[start..end]);

                        let module = file_to_module(db, definition.file(db))?;

                        if should_skip_import(db, module, *ty) {
                            return None;
                        }

                        let module_name = module.name(db).as_str();

                        dynamic_importer.import_symbol(
                            db,
                            ty,
                            module_name,
                            definition_name,
                            &details.label[start..end],
                        )
                    };

                    // Ok, this is the first type that claimed these bytes, give it the target
                    if start >= offset {
                        // Try to import the symbol and update the edit label if required
                        if let Some(qualified_name) = qualified_name(&mut dynamic_importer) {
                            let edit_start = (start.cast_signed() + edit_offset).cast_unsigned();
                            let edit_end = (end.cast_signed() + edit_offset).cast_unsigned();

                            edit_label.replace_range(edit_start..edit_end, &qualified_name);
                            edit_offset +=
                                qualified_name.len().cast_signed() - (end - start).cast_signed();
                        }

                        let target = ty.navigation_targets(db).into_iter().next();

                        // Always use original text for the label part
                        label_parts.push(
                            InlayHintLabelPart::new(&details.label[start..end]).with_target(target),
                        );
                        offset = end;
                    }
                }
                TypeDetail::SignatureStart
                | TypeDetail::SignatureEnd
                | TypeDetail::Parameter(_) => {
                    // Don't care about these
                }
            }
        }

        // "flush" the rest of the label without any target
        if offset < details.label.len() {
            label_parts.push(details.label[offset..details.label.len()].into());
        }

        let text_edits = if details.is_valid_syntax && allow_edits {
            let mut text_edits = vec![InlayHintTextEdit {
                range: TextRange::new(position, position),
                new_text: format!(": {edit_label}"),
            }];

            text_edits.extend(dynamic_importer.text_edits());

            text_edits
        } else {
            vec![]
        };

        Some(Self {
            position,
            kind: InlayHintKind::Type,
            label: InlayHintLabel { parts: label_parts },
            text_edits,
        })
    }

    fn call_argument_name(
        position: TextSize,
        name: &str,
        navigation_target: Option<NavigationTarget>,
    ) -> Self {
        let label_parts = vec![
            InlayHintLabelPart::new(name).with_target(navigation_target),
            "=".into(),
        ];

        Self {
            position,
            kind: InlayHintKind::CallArgumentName,
            label: InlayHintLabel { parts: label_parts },
            text_edits: vec![],
        }
    }

    pub fn display(&self) -> InlayHintDisplay<'_> {
        InlayHintDisplay { inlay_hint: self }
    }
}

#[derive(Debug, Clone)]
pub enum InlayHintKind {
    Type,
    CallArgumentName,
}

#[derive(Debug, Clone)]
pub struct InlayHintLabel {
    parts: Vec<InlayHintLabelPart>,
}

impl InlayHintLabel {
    pub fn parts(&self) -> &[InlayHintLabelPart] {
        &self.parts
    }

    pub fn into_parts(self) -> Vec<InlayHintLabelPart> {
        self.parts
    }
}

pub struct InlayHintDisplay<'a> {
    inlay_hint: &'a InlayHint,
}

impl fmt::Display for InlayHintDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.inlay_hint.label.parts {
            write!(f, "{}", part.text)?;
        }
        Ok(())
    }
}

#[derive(Default, Debug, Clone)]
pub struct InlayHintLabelPart {
    text: String,

    target: Option<NavigationTarget>,
}

impl InlayHintLabelPart {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            target: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn into_text(self) -> String {
        self.text
    }

    pub fn target(&self) -> Option<&NavigationTarget> {
        self.target.as_ref()
    }

    pub fn with_target(self, target: Option<NavigationTarget>) -> Self {
        Self { target, ..self }
    }
}

impl From<String> for InlayHintLabelPart {
    fn from(s: String) -> Self {
        Self {
            text: s,
            target: None,
        }
    }
}

impl From<&str> for InlayHintLabelPart {
    fn from(s: &str) -> Self {
        Self {
            text: s.to_string(),
            target: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InlayHintTextEdit {
    pub range: TextRange,
    pub new_text: String,
}

pub fn inlay_hints(
    db: &dyn Db,
    file: File,
    range: TextRange,
    settings: &InlayHintSettings,
) -> Vec<InlayHint> {
    let ast = parsed_module(db, file).load(db);

    let source = source_text(db, file);
    let stylist = Stylist::from_tokens(ast.tokens(), source.as_str());
    let importer = Importer::new(db, &stylist, file, source.as_str(), &ast);

    let mut visitor = InlayHintVisitor::new(db, file, importer, range, settings);

    visitor.visit_body(ast.suite());

    visitor.hints
}

/// Settings to control the behavior of inlay hints.
#[derive(Clone, Debug)]
pub struct InlayHintSettings {
    /// Whether to show variable type hints.
    ///
    /// For example, this would enable / disable hints like the ones quoted below:
    /// ```python
    /// x": Literal[1]" = 1
    /// ```
    pub variable_types: bool,

    /// Whether to show call argument names.
    ///
    /// For example, this would enable / disable hints like the ones quoted below:
    /// ```python
    /// def foo(x: int): pass
    /// foo("x="1)
    /// ```
    pub call_argument_names: bool,
    // Add any new setting that enables additional inlays to `any_enabled`.
}

impl InlayHintSettings {
    pub fn any_enabled(&self) -> bool {
        self.variable_types || self.call_argument_names
    }
}

impl Default for InlayHintSettings {
    fn default() -> Self {
        Self {
            variable_types: true,
            call_argument_names: true,
        }
    }
}

struct InlayHintImportContext<'a, 'db> {
    db: &'db dyn Db,
    file: File,
    importer: &'a Importer<'db>,
    dynamic_imports: &'a mut FxHashMap<DynamicallyImportedMember, ImportAction>,
}

struct InlayHintVisitor<'a, 'db> {
    db: &'db dyn Db,
    model: SemanticModel<'db>,
    /// Imports that we have already created.
    /// We store these imports so that we don't create multiple imports for the same symbol.
    dynamic_imports: FxHashMap<DynamicallyImportedMember, ImportAction>,
    importer: Importer<'db>,
    hints: Vec<InlayHint>,
    assignment_rhs: Option<&'a Expr>,
    range: TextRange,
    settings: &'a InlayHintSettings,
    in_no_edits_allowed: bool,
}

impl<'a, 'db> InlayHintVisitor<'a, 'db> {
    fn new(
        db: &'db dyn Db,
        file: File,
        importer: Importer<'db>,
        range: TextRange,
        settings: &'a InlayHintSettings,
    ) -> Self {
        Self {
            db,
            model: SemanticModel::new(db, file),
            dynamic_imports: FxHashMap::default(),
            importer,
            hints: Vec::new(),
            assignment_rhs: None,
            range,
            settings,
            in_no_edits_allowed: false,
        }
    }

    fn add_type_hint(&mut self, expr: &Expr, rhs: &Expr, ty: Type<'db>, allow_edits: bool) {
        if !self.settings.variable_types {
            return;
        }

        if is_ignored_variable_assignment_target(expr) {
            return;
        }

        let context = InlayHintImportContext {
            db: self.db,
            file: self.model.file(),
            importer: &self.importer,
            dynamic_imports: &mut self.dynamic_imports,
        };

        if let Some(inlay_hint) = InlayHint::variable_type(context, expr, rhs, ty, allow_edits) {
            self.hints.push(inlay_hint);
        }
    }

    fn add_call_argument_name(
        &mut self,
        position: TextSize,
        name: &str,
        navigation_target: Option<NavigationTarget>,
    ) -> bool {
        if !self.settings.call_argument_names {
            return false;
        }

        if name.starts_with('_') {
            return false;
        }

        let inlay_hint = InlayHint::call_argument_name(position, name, navigation_target);

        self.hints.push(inlay_hint);
        true
    }
}

impl<'a> SourceOrderVisitor<'a> for InlayHintVisitor<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        if self.range.intersect(node.range()).is_some() {
            TraversalSignal::Traverse
        } else {
            TraversalSignal::Skip
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let node = AnyNodeRef::from(stmt);

        if !self.enter_node(node).is_traverse() {
            return;
        }

        match stmt {
            Stmt::Assign(assign) => {
                if !type_hint_is_excessive_for_expr(&assign.value) {
                    self.assignment_rhs = Some(&*assign.value);
                }
                if !annotations_are_valid_syntax(assign) {
                    self.in_no_edits_allowed = true;
                }
                for target in &assign.targets {
                    self.visit_expr(target);
                }
                self.in_no_edits_allowed = false;
                self.assignment_rhs = None;

                self.visit_expr(&assign.value);

                return;
            }
            Stmt::Expr(expr) => {
                self.visit_expr(&expr.value);
                return;
            }
            // TODO
            Stmt::FunctionDef(_) => {}
            Stmt::For(_) => {}
            _ => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => {
                if let Some(rhs) = self.assignment_rhs {
                    if name.ctx.is_store() {
                        if let Some(ty) = expr.inferred_type(&self.model) {
                            self.add_type_hint(expr, rhs, ty, !self.in_no_edits_allowed);
                        }
                    }
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Attribute(attribute) => {
                if let Some(rhs) = self.assignment_rhs {
                    if attribute.ctx.is_store() {
                        if let Some(ty) = expr.inferred_type(&self.model) {
                            self.add_type_hint(expr, rhs, ty, !self.in_no_edits_allowed);
                        }
                    }
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Call(call) => {
                let details = inlay_hint_call_argument_details(self.db, &self.model, call)
                    .unwrap_or_default();

                self.visit_expr(&call.func);

                let mut last_editable_hint_index: Option<usize> = None;

                // `argument_names` is keyed by positional-arg index, not source-order index,
                // so track them separately to stay in sync after keyword args appear mid-call.
                let mut positional_index = 0;
                for arg_or_keyword in call.arguments.iter_source_order() {
                    if let ArgOrKeyword::Arg(argument) = arg_or_keyword {
                        if let Some((name, parameter_label_offset)) =
                            details.argument_names.get(&positional_index)
                            && !arg_matches_name(argument, name)
                        {
                            if self.add_call_argument_name(
                                arg_or_keyword.range().start(),
                                name,
                                parameter_label_offset.map(NavigationTarget::from),
                            ) {
                                if !argument.is_starred_expr() {
                                    last_editable_hint_index = Some(self.hints.len() - 1);
                                }
                            }
                        }

                        positional_index += 1;
                    }

                    self.visit_expr(arg_or_keyword.value());
                }

                // For the last positional argument, provide an edit to insert
                // the inlay hint.
                if let Some(index) = last_editable_hint_index {
                    let hint: &mut InlayHint = &mut self.hints[index];
                    hint.text_edits = vec![InlayHintTextEdit {
                        range: TextRange::empty(hint.position),
                        new_text: format!("{}=", hint.label.parts()[0].text()),
                    }];
                }
            }
            _ => {
                source_order::walk_expr(self, expr);
            }
        }
    }
}

/// Given a positional argument, check if the expression is the "same name"
/// as the function argument itself.
///
/// This allows us to filter out repetitive inlay hints like `x=x`, `x=y.x`, etc.,
/// and suppresses hints for arguments that are already explicit keyword arguments.
fn arg_matches_name(argument: &Expr, name: &str) -> bool {
    let mut expr = argument;
    loop {
        match expr {
            // `x=x(1, 2)` counts as a match, recurse for it
            Expr::Call(expr_call) => expr = &expr_call.func,
            // `x=x[0]` is a match, recurse for it
            Expr::Subscript(expr_subscript) => expr = &expr_subscript.value,
            // `x=x` is a match
            Expr::Name(expr_name) => return name_matches_parameter(expr_name.id.as_str(), name),
            // `x=y.x` is a match
            Expr::Attribute(expr_attribute) => {
                return name_matches_parameter(expr_attribute.attr.as_str(), name);
            }
            _ => return false,
        }
    }
}

/// Returns `true` when `argument_name` case-insensitively matches the parameter
/// name, or has the parameter name as a full underscore-separated prefix or
/// suffix. The parameter name is accepted in its raw spelling; leading and
/// trailing underscores are ignored before matching.
fn name_matches_parameter(argument_name: &str, parameter_name: &str) -> bool {
    let argument_name = argument_name.to_lowercase();
    let parameter_name = parameter_name.trim_matches('_').to_lowercase();

    argument_name == parameter_name
        || argument_name
            .strip_prefix(parameter_name.as_str())
            .is_some_and(|suffix| suffix.starts_with('_'))
        || argument_name
            .strip_suffix(parameter_name.as_str())
            .is_some_and(|prefix| prefix.ends_with('_'))
}

/// Given a function call, check if the expression is the "same name"
/// as the function being called.
///
/// This allows us to filter out reptitive inlay hints like `x: T = T(...)`.
/// While still allowing non-trivial ones like `x: T[U] = T()`.
fn call_matches_name(expr: &Expr, name: &str) -> bool {
    // Only care about function calls
    let Expr::Call(call) = expr else {
        return false;
    };

    match &*call.func {
        // `x: T = T()` is a match
        Expr::Name(expr_name) => expr_name.id.as_str() == name,
        // `x: T = a.T()` is a match
        Expr::Attribute(expr_attribute) => expr_attribute.attr.as_str() == name,
        _ => false,
    }
}

/// Given an expression that's the RHS of an assignment, would it be excessive to
/// emit an inlay type hint for the variable assigned to it?
///
/// This is used to suppress inlay hints for things like `x = 1`, `x, y = (1, 2)`, etc.
fn type_hint_is_excessive_for_expr(expr: &Expr) -> bool {
    match expr {
        // A tuple of all literals is excessive to typehint
        Expr::Tuple(expr_tuple) => expr_tuple.elts.iter().all(type_hint_is_excessive_for_expr),

        // Various Literal[...] types which are always excessive to hint
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::StringLiteral(_)
        // `None` isn't terribly verbose, but still redundant
        | Expr::NoneLiteral(_)
        // This one expands to `str` which isn't verbose but is redundant
        | Expr::FString(_)
        // This one expands to `Template` which isn't verbose but is redundant
        | Expr::TString(_)=> true,

        // You too `+1 and `-1`, get back here
        Expr::UnaryOp(ExprUnaryOp { op: UnaryOp::UAdd | UnaryOp::USub, operand, .. }) => matches!(**operand, Expr::NumberLiteral(_)),

        // Everything else is reasonable
        _ => false,
    }
}

fn should_skip_import(db: &dyn Db, module: ty_module_resolver::Module, ty: Type) -> bool {
    module.is_known(db, ty_module_resolver::KnownModule::Builtins) || ty.is_none(db)
}

fn annotations_are_valid_syntax(stmt_assign: &ruff_python_ast::StmtAssign) -> bool {
    if stmt_assign.targets.len() > 1 {
        return false;
    }

    if stmt_assign
        .targets
        .iter()
        .any(|target| matches!(target, Expr::Tuple(_)))
    {
        return false;
    }

    true
}

fn is_ignored_variable_assignment_target(expr: &Expr) -> bool {
    let Expr::Name(name) = expr else {
        return false;
    };

    let name = name.id.as_str();
    let is_dunder = name.starts_with("__") && name.ends_with("__") && name.len() > 4;

    name.starts_with('_') && !is_dunder
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DynamicallyImportedMember {
    module: String,
    name: String,
}

struct DynamicImporter<'a, 'db> {
    importer: &'a Importer<'db>,
    /// The expression node used to compute members in scope (lazily).
    scope_node: AnyNodeRef<'a>,
    scope_offset: TextSize,
    members: Option<MembersInScope<'db>>,
    dynamic_imports: &'a mut FxHashMap<DynamicallyImportedMember, ImportAction>,
    imported_members: Vec<DynamicallyImportedMember>,
}

impl<'a, 'db> DynamicImporter<'a, 'db> {
    fn new(
        importer: &'a Importer<'db>,
        expr: &'a Expr,
        dynamic_imports: &'a mut FxHashMap<DynamicallyImportedMember, ImportAction>,
    ) -> Self {
        Self {
            importer,
            scope_node: expr.into(),
            scope_offset: expr.range().start(),
            members: None,
            dynamic_imports,
            imported_members: Vec::new(),
        }
    }

    /// Attempts to import a given symbol.
    /// If the symbol in the text edit needs to be qualified, we return the qualified symbol text.
    fn import_symbol(
        &mut self,
        db: &dyn Db,
        ty: &Type,
        module_name: &str,
        symbol_name: &str,
        label_text: &str,
    ) -> Option<String> {
        use std::collections::hash_map::Entry;

        // Ensure members are computed before borrowing other fields.
        let members = self.members.get_or_insert_with(|| {
            self.importer
                .members_in_scope_at(self.scope_node, self.scope_offset)
        });

        // Check if the label is like `foo.A`
        let mut is_possibly_qualified_name = label_text.contains('.');

        if let Some(member) = members.find_member(symbol_name) {
            if member.ty.definition(db) == ty.definition(db) {
                return None;
            }

            // There is another member in scope with the same name,
            // so we need to qualify this so we don't reference the
            // in scope member.
            is_possibly_qualified_name = true;
        }

        let key = DynamicallyImportedMember {
            module: module_name.to_string(),
            name: symbol_name.to_string(),
        };

        match self.dynamic_imports.entry(key.clone()) {
            Entry::Vacant(entry) => {
                let request = if is_possibly_qualified_name {
                    ImportRequest::import(module_name, symbol_name).force()
                } else {
                    ImportRequest::import_from(module_name, symbol_name)
                };

                let import_action = self.importer.import(request, members);
                let action = entry.insert(import_action);

                self.imported_members.push(key);

                qualified_symbol_text(action).map(str::to_string)
            }
            Entry::Occupied(entry) => qualified_symbol_text(entry.get()).map(str::to_string),
        }
    }

    /// Builds the text edits from all collected imports.
    fn text_edits(&self) -> Vec<InlayHintTextEdit> {
        self.imported_members
            .iter()
            .filter_map(|member| self.dynamic_imports.get(member))
            .filter_map(|import_action| {
                import_action.import().and_then(|edit| {
                    edit.content().map(|content| InlayHintTextEdit {
                        range: edit.range(),
                        new_text: content.to_string(),
                    })
                })
            })
            .collect()
    }
}

/// If the import action requires qualifying the symbol (e.g. `import foo` instead of
/// `from foo import A`), returns the qualified symbol text. Otherwise returns `None`.
fn qualified_symbol_text(import_action: &ImportAction) -> Option<&str> {
    if import_action.import().is_some() {
        return None;
    }
    Some(import_action.symbol_text())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::NavigationTarget;
    use crate::tests::IntoDiagnostic;
    use insta::{assert_snapshot, internals::SettingsBindDropGuard};
    use itertools::Itertools;
    use ruff_db::{
        diagnostic::{
            Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig,
            LintName, Severity, Span, SubDiagnostic, SubDiagnosticSeverity,
        },
        files::{File, FileRange, system_path_to_file},
        source::source_text,
    };
    use ruff_diagnostics::{Edit, Fix};
    use ruff_python_ast::PySourceType;
    use ruff_python_parser::parse_unchecked_source;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::{TextLen, TextSize};

    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ty_project::ProjectMetadata;

    pub(super) fn inlay_hint_test(source: &str) -> InlayHintTest {
        const START: &str = "<START>";
        const END: &str = "<END>";

        let mut db = ty_project::TestDb::new(ProjectMetadata::new(
            "test".into(),
            SystemPathBuf::from("/"),
        ));

        db.init_program().unwrap();

        let source = dedent(source);

        let start = source.find(START);
        let end = source
            .find(END)
            .map(|x| if start.is_some() { x - START.len() } else { x })
            .unwrap_or(source.len());

        let range = TextRange::new(
            TextSize::try_from(start.unwrap_or_default()).unwrap(),
            TextSize::try_from(end).unwrap(),
        );

        let source = source.replace(START, "");
        let source = source.replace(END, "");

        db.write_file("main.py", source)
            .expect("write to memory file system to be successful");

        let file = system_path_to_file(&db, "main.py").expect("newly written file to existing");

        let mut insta_settings = insta::Settings::clone_current();
        insta_settings.add_filter(r#"\\(\w\w|\.|")"#, "/$1");
        // Filter out TODO types because they are different between debug and release builds.
        insta_settings.add_filter(r"@Todo\(.+\)", "@Todo");

        let insta_settings_guard = insta_settings.bind_to_scope();

        InlayHintTest {
            db,
            file,
            range,
            _insta_settings_guard: insta_settings_guard,
        }
    }

    pub(super) struct InlayHintTest {
        pub(super) db: ty_project::TestDb,
        pub(super) file: File,
        pub(super) range: TextRange,
        _insta_settings_guard: SettingsBindDropGuard,
    }

    impl InlayHintTest {
        /// Returns the inlay hints for the given test case.
        ///
        /// All inlay hints are generated using the applicable settings. Use
        /// [`inlay_hints_with_settings`] to generate hints with custom settings.
        ///
        /// [`inlay_hints_with_settings`]: Self::inlay_hints_with_settings
        fn inlay_hints(&mut self) -> String {
            self.inlay_hints_with_settings(&InlayHintSettings::default())
        }

        fn with_extra_file(&mut self, file_name: &str, content: &str) {
            self.db.write_file(file_name, content).unwrap();
        }

        /// Returns the inlay hints for the given test case with custom settings.
        fn inlay_hints_with_settings(&mut self, settings: &InlayHintSettings) -> String {
            let hints = inlay_hints(&self.db, self.file, self.range, settings);

            let mut inlay_hint_buf = source_text(&self.db, self.file).as_str().to_string();
            let mut text_edit_buf = inlay_hint_buf.clone();
            let source_has_errors =
                parse_unchecked_source(&text_edit_buf, PySourceType::Python).has_invalid_syntax();

            let mut tbd_diagnostics = Vec::new();

            let mut offset = 0;

            let mut all_edits = Vec::new();

            for hint in hints {
                let end_position = hint.position.to_usize() + offset;
                let mut hint_str = "[".to_string();

                for part in hint.label.parts() {
                    if let Some(target) = part.target().cloned() {
                        let part_position = u32::try_from(end_position + hint_str.len()).unwrap();
                        let part_len = u32::try_from(part.text().len()).unwrap();
                        let label_range =
                            TextRange::at(TextSize::new(part_position), TextSize::new(part_len));
                        tbd_diagnostics.push((label_range, target));
                    }
                    hint_str.push_str(part.text());
                }

                all_edits.extend(hint.text_edits);

                hint_str.push(']');
                offset += hint_str.len();

                inlay_hint_buf.insert_str(end_position, &hint_str);
            }
            let mut edit_offset = TextSize::default();

            for edit in all_edits.iter().sorted_by_key(|edit| edit.range.start()) {
                let updated_range = edit.range + edit_offset;
                text_edit_buf.replace_range(updated_range.to_std_range(), &edit.new_text);
                edit_offset += edit.new_text.text_len() - edit.range.len();
            }

            let edited = parse_unchecked_source(&text_edit_buf, PySourceType::Python);
            if edited.has_invalid_syntax() && !source_has_errors {
                let syntax_errors = edited.errors().iter().map(|error| &error.error).join("\n");

                panic!(
                    "Fixed source has a syntax error where the source document does not. This is a bug in one of the generated inlay hint edits:
{syntax_errors}
Source with applied edits:
{text_edit_buf}"
                );
            }
            self.db.write_file("main2.py", &inlay_hint_buf).unwrap();
            let inlayed_file =
                system_path_to_file(&self.db, "main2.py").expect("newly written file to existing");

            let location_diagnostics = tbd_diagnostics.into_iter().map(|(label_range, target)| {
                InlayHintLocationDiagnostic::new(FileRange::new(inlayed_file, label_range), &target)
            });

            let mut rendered_diagnostics = location_diagnostics
                .map(|diagnostic| self.render_diagnostic(diagnostic))
                .join("");

            if !rendered_diagnostics.is_empty() {
                rendered_diagnostics = format!(
                    "{}{}",
                    crate::MarkupKind::PlainText.horizontal_line(),
                    rendered_diagnostics
                        .strip_suffix("\n")
                        .unwrap_or(&rendered_diagnostics)
                );
            }

            let fixes = if let Some((first_edit, rest)) = all_edits.split_first() {
                let edit_diagnostic = InlayHintEditDiagnostic::new(self.file, first_edit, rest);
                let text_edit_buf = self.render_diagnostic(edit_diagnostic);

                format!(
                    "{}{}",
                    crate::MarkupKind::PlainText.horizontal_line(),
                    text_edit_buf
                )
            } else {
                String::new()
            };

            format!("{inlay_hint_buf}{rendered_diagnostics}{fixes}")
        }

        fn render_diagnostic<D>(&self, diagnostic: D) -> String
        where
            D: IntoDiagnostic,
        {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::new("ty")
                .color(false)
                .show_fix_diff(true)
                .context(0)
                .format(DiagnosticFormat::Full);

            let diag = diagnostic.into_diagnostic();
            write!(buf, "{}", diag.display(&self.db, &config)).unwrap();

            buf
        }
    }

    #[test]
    fn test_assign_statement() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x = 1
            y = x
            z = i(1)
            w = z
            aa = b'foo'
            bb = aa
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def i(x: int, /) -> int:
            return x

        x = 1
        y[: Literal[1]] = x
        z[: int] = i(1)
        w[: int] = z
        aa = b'foo'
        bb[: Literal[b"foo"]] = aa

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:6:5
          |
        6 | y[: Literal[1]] = x
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:6:13
          |
        6 | y[: Literal[1]] = x
          |             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:7:5
          |
        7 | z[: int] = i(1)
          |     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:5
          |
        8 | w[: int] = z
          |     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
          --> main2.py:10:6
           |
        10 | bb[: Literal[b"foo"]] = aa
           |      ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:1448:7
             |
        1448 | class bytes(Sequence[int]):
             |       ^^^^^
             |
        info: Source
          --> main2.py:10:14
           |
        10 | bb[: Literal[b"foo"]] = aa
           |              ^^^^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1  + from typing import Literal
        2  |
        3  | def i(x: int, /) -> int:
        4  |     return x
        5  |
        6  | x = 1
           - y = x
           - z = i(1)
           - w = z
        7  + y: Literal[1] = x
        8  + z: int = i(1)
        9  + w: int = z
        10 | aa = b'foo'
           - bb = aa
        11 + bb: Literal[b"foo"] = aa
        "#);
    }

    #[test]
    fn test_unpacked_tuple_assignment() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x1, y1 = (1, 'abc')
            x2, y2 = (x1, y1)
            x3, y3 = (i(1), s('abc'))
            x4, y4 = (x3, y3)
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x1, y1 = (1, 'abc')
        x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
        x3[: int], y3[: str] = (i(1), s('abc'))
        x4[: int], y4[: str] = (x3, y3)

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:6
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
          |      ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:14
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
          |              ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:24
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
          |                        ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:32
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
          |                                ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:9:6
          |
        9 | x3[: int], y3[: str] = (i(1), s('abc'))
          |      ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:9:17
          |
        9 | x3[: int], y3[: str] = (i(1), s('abc'))
          |                 ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:10:6
           |
        10 | x4[: int], y4[: str] = (x3, y3)
           |      ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:10:17
           |
        10 | x4[: int], y4[: str] = (x3, y3)
           |                 ^^^
           |
        "#);
    }

    #[test]
    fn test_starred_unpacked_tuple_assignment() {
        let mut test = inlay_hint_test(
            "
            def foo(x: tuple[int, ...]):
                (a, *b) = x
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: tuple[int, ...]):
            (a[: int], *b[: list[int]]) = x

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:3:10
          |
        3 |     (a[: int], *b[: list[int]]) = x
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:3:21
          |
        3 |     (a[: int], *b[: list[int]]) = x
          |                     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:3:26
          |
        3 |     (a[: int], *b[: list[int]]) = x
          |                          ^^^
          |
        ");
    }

    #[test]
    fn test_leading_underscore_variable_assignment_has_no_type_inlay_hint() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            _ = i(1)
            _ignored = i(1)
            __ignored = i(1)
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def i(x: int, /) -> int:
            return x

        _ = i(1)
        _ignored = i(1)
        __ignored = i(1)
        ");
    }

    #[test]
    fn test_leading_underscore_variable_in_tuple_assignment_has_no_type_inlay_hint() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x, _ignored = (i(1), s('abc'))
            __ignored, y = (i(1), s('abc'))
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x[: int], _ignored = (i(1), s('abc'))
        __ignored, y[: str] = (i(1), s('abc'))

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:7:5
          |
        7 | x[: int], _ignored = (i(1), s('abc'))
          |     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:16
          |
        8 | __ignored, y[: str] = (i(1), s('abc'))
          |                ^^^
          |
        ");
    }

    #[test]
    fn test_dunder_variable_assignment_has_type_inlay_hint() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            __special__ = i(1)
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def i(x: int, /) -> int:
            return x

        __special__[: int] = i(1)

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:5:15
          |
        5 | __special__[: int] = i(1)
          |               ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        2 | def i(x: int, /) -> int:
        3 |     return x
        4 |
          - __special__ = i(1)
        5 + __special__: int = i(1)
        ");
    }

    #[test]
    fn test_multiple_assignment() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x1, y1 = 1, 'abc'
            x2, y2 = x1, y1
            x3, y3 = i(1), s('abc')
            x4, y4 = x3, y3
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x1, y1 = 1, 'abc'
        x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
        x3[: int], y3[: str] = i(1), s('abc')
        x4[: int], y4[: str] = x3, y3

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:6
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
          |      ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:14
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
          |              ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:24
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
          |                        ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:32
          |
        8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
          |                                ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:9:6
          |
        9 | x3[: int], y3[: str] = i(1), s('abc')
          |      ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:9:17
          |
        9 | x3[: int], y3[: str] = i(1), s('abc')
          |                 ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:10:6
           |
        10 | x4[: int], y4[: str] = x3, y3
           |      ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:10:17
           |
        10 | x4[: int], y4[: str] = x3, y3
           |                 ^^^
           |
        "#);
    }

    #[test]
    fn test_tuple_assignment() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x = (1, 'abc')
            y = x
            z = (i(1), s('abc'))
            w = z
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x = (1, 'abc')
        y[: tuple[Literal[1], Literal["abc"]]] = x
        z[: tuple[int, str]] = (i(1), s('abc'))
        w[: tuple[int, str]] = z

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2722:7
             |
        2722 | class tuple(Sequence[_T_co]):
             |       ^^^^^
             |
        info: Source
         --> main2.py:8:5
          |
        8 | y[: tuple[Literal[1], Literal["abc"]]] = x
          |     ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:11
          |
        8 | y[: tuple[Literal[1], Literal["abc"]]] = x
          |           ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:19
          |
        8 | y[: tuple[Literal[1], Literal["abc"]]] = x
          |                   ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:23
          |
        8 | y[: tuple[Literal[1], Literal["abc"]]] = x
          |                       ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:31
          |
        8 | y[: tuple[Literal[1], Literal["abc"]]] = x
          |                               ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2722:7
             |
        2722 | class tuple(Sequence[_T_co]):
             |       ^^^^^
             |
        info: Source
         --> main2.py:9:5
          |
        9 | z[: tuple[int, str]] = (i(1), s('abc'))
          |     ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:9:11
          |
        9 | z[: tuple[int, str]] = (i(1), s('abc'))
          |           ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:9:16
          |
        9 | z[: tuple[int, str]] = (i(1), s('abc'))
          |                ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2722:7
             |
        2722 | class tuple(Sequence[_T_co]):
             |       ^^^^^
             |
        info: Source
          --> main2.py:10:5
           |
        10 | w[: tuple[int, str]] = z
           |     ^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:10:11
           |
        10 | w[: tuple[int, str]] = z
           |           ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:10:16
           |
        10 | w[: tuple[int, str]] = z
           |                ^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1  + from typing import Literal
        2  |
        3  | def i(x: int, /) -> int:
        4  |     return x
        --------------------------------------------------------------------------------
        6  |     return x
        7  |
        8  | x = (1, 'abc')
           - y = x
           - z = (i(1), s('abc'))
           - w = z
        9  + y: tuple[Literal[1], Literal["abc"]] = x
        10 + z: tuple[int, str] = (i(1), s('abc'))
        11 + w: tuple[int, str] = z
        "#);
    }

    #[test]
    fn test_nested_tuple_assignment() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x1, (y1, z1) = (1, ('abc', 2))
            x2, (y2, z2) = (x1, (y1, z1))
            x3, (y3, z3) = (i(1), (s('abc'), i(2)))
            x4, (y4, z4) = (x3, (y3, z3))",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x1, (y1, z1) = (1, ('abc', 2))
        x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
        x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:6
          |
        8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
          |      ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:14
          |
        8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
          |              ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:25
          |
        8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
          |                         ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:33
          |
        8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
          |                                 ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:47
          |
        8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
          |                                               ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:55
          |
        8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
          |                                                       ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:9:6
          |
        9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
          |      ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:9:18
          |
        9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
          |                  ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:9:29
          |
        9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
          |                             ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:10:6
           |
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |      ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:10:18
           |
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |                  ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:10:29
           |
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |                             ^^^
           |
        "#);
    }

    #[test]
    fn test_assign_statement_with_type_annotation() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x: int = 1
            y = x
            z: int = i(1)
            w = z",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def i(x: int, /) -> int:
            return x

        x: int = 1
        y[: Literal[1]] = x
        z: int = i(1)
        w[: int] = z
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:6:5
          |
        6 | y[: Literal[1]] = x
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:6:13
          |
        6 | y[: Literal[1]] = x
          |             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:5
          |
        8 | w[: int] = z
          |     ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + from typing import Literal
        2 |
        3 | def i(x: int, /) -> int:
        4 |     return x
        5 |
        6 | x: int = 1
          - y = x
        7 + y: Literal[1] = x
        8 | z: int = i(1)
          - w = z
        9 + w: int = z
        ");
    }

    #[test]
    fn test_assign_statement_out_of_range() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            <START>x = i(1)<END>
            z = x",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def i(x: int, /) -> int:
            return x
        x[: int] = i(1)
        z = x
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:5
          |
        4 | x[: int] = i(1)
          |     ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def i(x: int, /) -> int:
        3 |     return x
          - x = i(1)
        4 + x: int = i(1)
        5 | z = x
        ");
    }

    #[test]
    fn test_assign_attribute_of_instance() {
        let mut test = inlay_hint_test(
            "
            class A:
                def __init__(self, y):
                    self.x = int(1)
                    self.y = y

            a = A(2)
            a.y = int(3)
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class A:
            def __init__(self, y):
                self.x = int(1)
                self.y[: Unknown] = y

        a = A([y=]2)
        a.y = int(3)

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:14:1
           |
        14 | Unknown: _SpecialForm
           | ^^^^^^^
           |
        info: Source
         --> main2.py:5:18
          |
        5 |         self.y[: Unknown] = y
          |                  ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, y):
          |                        ^
          |
        info: Source
         --> main2.py:7:8
          |
        7 | a = A([y=]2)
          |        ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + from ty_extensions import Unknown
        2 |
        3 | class A:
        4 |     def __init__(self, y):
        5 |         self.x = int(1)
          -         self.y = y
        6 +         self.y: Unknown = y
        7 |
          - a = A(2)
        8 + a = A(y=2)
        9 | a.y = int(3)
        ");
    }

    #[test]
    fn test_match_name_binding() {
        let mut test = inlay_hint_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ab]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def my_func(command: str):
            match command.split():
                case ["get", ab]:
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_match_rest_binding() {
        let mut test = inlay_hint_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *ab]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def my_func(command: str):
            match command.split():
                case ["get", *ab]:
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_match_as_binding() {
        let mut test = inlay_hint_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as ab]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def my_func(command: str):
            match command.split():
                case ["get", ("a" | "b") as ab]:
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_match_keyword_binding() {
        let mut test = inlay_hint_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        class Click:
            __match_args__ = ("position", "button")
            def __init__(self, pos, btn):
                self.position: int = pos
                self.button: str = btn

        def my_func(event: Click):
            match event:
                case Click(x, button=ab):
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_typevar_name_binding() {
        let mut test = inlay_hint_test(
            r#"
            type Alias1[AB: int = bool] = tuple[AB, list[AB]]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        type Alias1[AB: int = bool] = tuple[AB, list[AB]]
        ");
    }

    #[test]
    fn test_typevar_spec_binding() {
        let mut test = inlay_hint_test(
            r#"
            from typing import Callable
            type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import Callable
        type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
        ");
    }

    #[test]
    fn test_typevar_tuple_binding() {
        let mut test = inlay_hint_test(
            r#"
            type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
        ");
    }

    #[test]
    fn test_many_literals() {
        let mut test = inlay_hint_test(
            r#"
            a = 1
            b = 1.0
            c = True
            d = None
            e = "hello"
            f = 'there'
            g = f"{e} {f}"
            h = t"wow %d"
            i = b'\x00'
            j = +1
            k = -1.0
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        a = 1
        b = 1.0
        c = True
        d = None
        e = "hello"
        f = 'there'
        g = f"{e} {f}"
        h = t"wow %d"
        i = b'/x00'
        j = +1
        k = -1.0
        "#);
    }

    #[test]
    fn test_many_literals_tuple() {
        let mut test = inlay_hint_test(
            r#"
            a = (1, 2)
            b = (1.0, 2.0)
            c = (True, False)
            d = (None, None)
            e = ("hel", "lo")
            f = ('the', 're')
            g = (f"{ft}", f"{ft}")
            h = (t"wow %d", t"wow %d")
            i = (b'\x01', b'\x02')
            j = (+1, +2.0)
            k = (-1, -2.0)
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        a = (1, 2)
        b = (1.0, 2.0)
        c = (True, False)
        d = (None, None)
        e = ("hel", "lo")
        f = ('the', 're')
        g = (f"{ft}", f"{ft}")
        h = (t"wow %d", t"wow %d")
        i = (b'/x01', b'/x02')
        j = (+1, +2.0)
        k = (-1, -2.0)
        "#);
    }

    #[test]
    fn test_many_literals_unpacked_tuple() {
        let mut test = inlay_hint_test(
            r#"
            a1, a2 = (1, 2)
            b1, b2 = (1.0, 2.0)
            c1, c2 = (True, False)
            d1, d2 = (None, None)
            e1, e2 = ("hel", "lo")
            f1, f2 = ('the', 're')
            g1, g2 = (f"{ft}", f"{ft}")
            h1, h2 = (t"wow %d", t"wow %d")
            i1, i2 = (b'\x01', b'\x02')
            j1, j2 = (+1, +2.0)
            k1, k2 = (-1, -2.0)
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        a1, a2 = (1, 2)
        b1, b2 = (1.0, 2.0)
        c1, c2 = (True, False)
        d1, d2 = (None, None)
        e1, e2 = ("hel", "lo")
        f1, f2 = ('the', 're')
        g1, g2 = (f"{ft}", f"{ft}")
        h1, h2 = (t"wow %d", t"wow %d")
        i1, i2 = (b'/x01', b'/x02')
        j1, j2 = (+1, +2.0)
        k1, k2 = (-1, -2.0)
        "#);
    }

    #[test]
    fn test_many_literals_multiple() {
        let mut test = inlay_hint_test(
            r#"
            a1, a2 = 1, 2
            b1, b2 = 1.0, 2.0
            c1, c2 = True, False
            d1, d2 = None, None
            e1, e2 = "hel", "lo"
            f1, f2 = 'the', 're'
            g1, g2 = f"{ft}", f"{ft}"
            h1, h2 = t"wow %d", t"wow %d"
            i1, i2 = b'\x01', b'\x02'
            j1, j2 = +1, +2.0
            k1, k2 = -1, -2.0
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        a1, a2 = 1, 2
        b1, b2 = 1.0, 2.0
        c1, c2 = True, False
        d1, d2 = None, None
        e1, e2 = "hel", "lo"
        f1, f2 = 'the', 're'
        g1, g2 = f"{ft}", f"{ft}"
        h1, h2 = t"wow %d", t"wow %d"
        i1, i2 = b'/x01', b'/x02'
        j1, j2 = +1, +2.0
        k1, k2 = -1, -2.0
        "#);
    }

    #[test]
    fn test_many_literals_list() {
        let mut test = inlay_hint_test(
            r#"
            a = [1, 2]
            b = [1.0, 2.0]
            c = [True, False]
            d = [None, None]
            e = ["hel", "lo"]
            f = ['the', 're']
            g = [f"{ft}", f"{ft}"]
            h = [t"wow %d", t"wow %d"]
            i = [b'\x01', b'\x02']
            j = [+1, +2.0]
            k = [-1, -2.0]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        a[: list[int]] = [1, 2]
        b[: list[int | float]] = [1.0, 2.0]
        c[: list[bool]] = [True, False]
        d[: list[None | Unknown]] = [None, None]
        e[: list[str]] = ["hel", "lo"]
        f[: list[str]] = ['the', 're']
        g[: list[str]] = [f"{ft}", f"{ft}"]
        h[: list[Template]] = [t"wow %d", t"wow %d"]
        i[: list[bytes]] = [b'/x01', b'/x02']
        j[: list[int | float]] = [+1, +2.0]
        k[: list[int | float]] = [-1, -2.0]

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:2:5
          |
        2 | a[: list[int]] = [1, 2]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:2:10
          |
        2 | a[: list[int]] = [1, 2]
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:3:5
          |
        3 | b[: list[int | float]] = [1.0, 2.0]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:3:10
          |
        3 | b[: list[int | float]] = [1.0, 2.0]
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:661:7
            |
        661 | class float:
            |       ^^^^^
            |
        info: Source
         --> main2.py:3:16
          |
        3 | b[: list[int | float]] = [1.0, 2.0]
          |                ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:5
          |
        4 | c[: list[bool]] = [True, False]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2618:7
             |
        2618 | class bool(int):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:10
          |
        4 | c[: list[bool]] = [True, False]
          |          ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:5:5
          |
        5 | d[: list[None | Unknown]] = [None, None]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:969:11
            |
        969 |     class NoneType:
            |           ^^^^^^^^
            |
        info: Source
         --> main2.py:5:10
          |
        5 | d[: list[None | Unknown]] = [None, None]
          |          ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:14:1
           |
        14 | Unknown: _SpecialForm
           | ^^^^^^^
           |
        info: Source
         --> main2.py:5:17
          |
        5 | d[: list[None | Unknown]] = [None, None]
          |                 ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:6:5
          |
        6 | e[: list[str]] = ["hel", "lo"]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:6:10
          |
        6 | e[: list[str]] = ["hel", "lo"]
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:7:5
          |
        7 | f[: list[str]] = ['the', 're']
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:7:10
          |
        7 | f[: list[str]] = ['the', 're']
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:8:5
          |
        8 | g[: list[str]] = [f"{ft}", f"{ft}"]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:10
          |
        8 | g[: list[str]] = [f"{ft}", f"{ft}"]
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:9:5
          |
        9 | h[: list[Template]] = [t"wow %d", t"wow %d"]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/string/templatelib.pyi:10:7
           |
        10 | class Template:  # TODO: consider making `Template` generic on `TypeVarTuple`
           |       ^^^^^^^^
           |
        info: Source
         --> main2.py:9:10
          |
        9 | h[: list[Template]] = [t"wow %d", t"wow %d"]
          |          ^^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
          --> main2.py:10:5
           |
        10 | i[: list[bytes]] = [b'/x01', b'/x02']
           |     ^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:1448:7
             |
        1448 | class bytes(Sequence[int]):
             |       ^^^^^
             |
        info: Source
          --> main2.py:10:10
           |
        10 | i[: list[bytes]] = [b'/x01', b'/x02']
           |          ^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
          --> main2.py:11:5
           |
        11 | j[: list[int | float]] = [+1, +2.0]
           |     ^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:11:10
           |
        11 | j[: list[int | float]] = [+1, +2.0]
           |          ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:661:7
            |
        661 | class float:
            |       ^^^^^
            |
        info: Source
          --> main2.py:11:16
           |
        11 | j[: list[int | float]] = [+1, +2.0]
           |                ^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
          --> main2.py:12:5
           |
        12 | k[: list[int | float]] = [-1, -2.0]
           |     ^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:12:10
           |
        12 | k[: list[int | float]] = [-1, -2.0]
           |          ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:661:7
            |
        661 | class float:
            |       ^^^^^
            |
        info: Source
          --> main2.py:12:16
           |
        12 | k[: list[int | float]] = [-1, -2.0]
           |                ^^^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1  + from ty_extensions import Unknown
        2  + from string.templatelib import Template
        3  |
           - a = [1, 2]
           - b = [1.0, 2.0]
           - c = [True, False]
           - d = [None, None]
           - e = ["hel", "lo"]
           - f = ['the', 're']
           - g = [f"{ft}", f"{ft}"]
           - h = [t"wow %d", t"wow %d"]
           - i = [b'/x01', b'/x02']
           - j = [+1, +2.0]
           - k = [-1, -2.0]
        4  + a: list[int] = [1, 2]
        5  + b: list[int | float] = [1.0, 2.0]
        6  + c: list[bool] = [True, False]
        7  + d: list[None | Unknown] = [None, None]
        8  + e: list[str] = ["hel", "lo"]
        9  + f: list[str] = ['the', 're']
        10 + g: list[str] = [f"{ft}", f"{ft}"]
        11 + h: list[Template] = [t"wow %d", t"wow %d"]
        12 + i: list[bytes] = [b'/x01', b'/x02']
        13 + j: list[int | float] = [+1, +2.0]
        14 + k: list[int | float] = [-1, -2.0]
        "#);
    }

    #[test]
    fn test_enum_literal() {
        let mut test = inlay_hint_test(
            r#"
            from enum import Enum

            class Color(Enum):
                RED = 1
                BLUE = 2

            x = Color.RED
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from enum import Enum

        class Color(Enum):
            RED = 1
            BLUE = 2

        x[: Literal[Color.RED]] = Color.RED

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:8:5
          |
        8 | x[: Literal[Color.RED]] = Color.RED
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:7
          |
        4 | class Color(Enum):
          |       ^^^^^
          |
        info: Source
         --> main2.py:8:13
          |
        8 | x[: Literal[Color.RED]] = Color.RED
          |             ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:5
          |
        5 |     RED = 1
          |     ^^^
          |
        info: Source
         --> main2.py:8:19
          |
        8 | x[: Literal[Color.RED]] = Color.RED
          |                   ^^^
          |
        ");
    }

    #[test]
    fn test_simple_init_call() {
        let mut test = inlay_hint_test(
            r#"
            class MyClass:
                def __init__(self):
                    self.x: int = 1

            x = MyClass()
            y = (MyClass(), MyClass())
            a, b = MyClass(), MyClass()
            c, d = (MyClass(), MyClass())
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        class MyClass:
            def __init__(self):
                self.x: int = 1

        x = MyClass()
        y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        c[: MyClass], d[: MyClass] = (MyClass(), MyClass())

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2722:7
             |
        2722 | class tuple(Sequence[_T_co]):
             |       ^^^^^
             |
        info: Source
         --> main2.py:7:5
          |
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
          |     ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:7:11
          |
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
          |           ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:7:20
          |
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
          |                    ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:8:5
          |
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:8:19
          |
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
          |                   ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:9:5
          |
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:9:19
          |
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |                   ^^^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        4 |         self.x: int = 1
        5 |
        6 | x = MyClass()
          - y = (MyClass(), MyClass())
        7 + y: tuple[MyClass, MyClass] = (MyClass(), MyClass())
        8 | a, b = MyClass(), MyClass()
        9 | c, d = (MyClass(), MyClass())
        ");
    }

    #[test]
    fn test_generic_init_call() {
        let mut test = inlay_hint_test(
            r#"
            class MyClass[T, U]:
                def __init__(self, x: list[T], y: tuple[U, U]):
                    self.x = x
                    self.y = y

            x = MyClass([42], ("a", "b"))
            y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
            a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
            c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        class MyClass[T, U]:
            def __init__(self, x: list[T], y: tuple[U, U]):
                self.x[: list[T@MyClass]] = x
                self.y[: tuple[U@MyClass, U@MyClass]] = y

        x[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b"))
        y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
        a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
        c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:18
          |
        4 |         self.x[: list[T@MyClass]] = x
          |                  ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2722:7
             |
        2722 | class tuple(Sequence[_T_co]):
             |       ^^^^^
             |
        info: Source
         --> main2.py:5:18
          |
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
          |                  ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:7:5
          |
        7 | x[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:7:13
          |
        7 | x[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |             ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:7:18
          |
        7 | x[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |                  ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
          |
        info: Source
         --> main2.py:7:35
          |
        7 | x[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |                                   ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
          |
        info: Source
         --> main2.py:7:45
          |
        7 | x[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |                                             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2722:7
             |
        2722 | class tuple(Sequence[_T_co]):
             |       ^^^^^
             |
        info: Source
         --> main2.py:8:5
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |     ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:8:11
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |           ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:19
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                   ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:24
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                        ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:8:30
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                              ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:8:38
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                                      ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:8:43
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                                           ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
          |
        info: Source
         --> main2.py:8:62
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                                                              ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
          |
        info: Source
         --> main2.py:8:72
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                                                                        ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
          |
        info: Source
         --> main2.py:8:97
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                                                                                                 ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
          |
        info: Source
         --> main2.py:8:107
          |
        8 | y[: tuple[MyClass[int, str], MyClass[int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
          |                                                                                                           ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:9:5
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:9:13
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |             ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:9:18
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                  ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
          |
        info: Source
         --> main2.py:9:29
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                             ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:9:37
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                                     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:9:42
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                                          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
          |
        info: Source
         --> main2.py:9:59
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                                                           ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
          |
        info: Source
         --> main2.py:9:69
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                                                                     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
          |
        info: Source
         --> main2.py:9:94
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                                                                                              ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
          |
        info: Source
         --> main2.py:9:104
          |
        9 | a[: MyClass[int, str]], b[: MyClass[int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
          |                                                                                                        ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
          |
        info: Source
          --> main2.py:10:5
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |     ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:10:13
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |             ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:10:18
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                  ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
          |
        info: Source
          --> main2.py:10:29
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                             ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:10:37
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                                     ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:10:42
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                                          ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
          |
        info: Source
          --> main2.py:10:60
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                                                            ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
          |
        info: Source
          --> main2.py:10:70
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                                                                      ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
          |
        info: Source
          --> main2.py:10:95
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                                                                                               ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
          |
        info: Source
          --> main2.py:10:105
           |
        10 | c[: MyClass[int, str]], d[: MyClass[int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
           |                                                                                                         ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        4  |         self.x = x
        5  |         self.y = y
        6  |
           - x = MyClass([42], ("a", "b"))
           - y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           - a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
           - c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
        7  + x: MyClass[int, str] = MyClass([42], y=("a", "b"))
        8  + y: tuple[MyClass[int, str], MyClass[int, str]] = (MyClass([42], y=("a", "b")), MyClass([42], y=("a", "b")))
        9  + a, b = MyClass([42], y=("a", "b")), MyClass([42], y=("a", "b"))
        10 + c, d = (MyClass([42], y=("a", "b")), MyClass([42], y=("a", "b")))
        "#);
    }

    #[test]
    fn test_disabled_variable_types() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x = i(1)
            ",
        );

        assert_snapshot!(
            test.inlay_hints_with_settings(&InlayHintSettings {
                variable_types: false,
                ..Default::default()
            }),
            @"

        def i(x: int, /) -> int:
            return x

        x = i(1)
        "
        );
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int): pass
        foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
          |
        info: Source
         --> main2.py:3:6
          |
        3 | foo([x=]1)
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int): pass
          - foo(1)
        3 + foo(x=1)
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_name() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int): pass
            x = 1
            y = 2
            foo(x)
            foo(y)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int): pass
        x = 1
        y = 2
        foo(x)
        foo([x=]y)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
          |
        info: Source
         --> main2.py:6:6
          |
        6 | foo([x=]y)
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        3 | x = 1
        4 | y = 2
        5 | foo(x)
          - foo(y)
        6 + foo(x=y)
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_attribute() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                    self.x: int = 1
                    self.y: int = 2
            val = MyClass()

            foo(val.x)
            foo(val.y)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
                self.x: int = 1
                self.y: int = 2
        val = MyClass()

        foo(val.x)
        foo([x=]val.y)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
          |
        info: Source
          --> main2.py:10:6
           |
        10 | foo([x=]val.y)
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        7  | val = MyClass()
        8  |
        9  | foo(val.x)
           - foo(val.y)
        10 + foo(x=val.y)
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_attribute_not() {
        // This one checks that we don't allow elide `x=` for `x.y`
        let mut test = inlay_hint_test(
            "
            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                    self.x: int = 1
                    self.y: int = 2
            x = MyClass()

            foo(x.x)
            foo(x.y)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
                self.x: int = 1
                self.y: int = 2
        x = MyClass()

        foo(x.x)
        foo([x=]x.y)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
          |
        info: Source
          --> main2.py:10:6
           |
        10 | foo([x=]x.y)
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        7  | x = MyClass()
        8  |
        9  | foo(x.x)
           - foo(x.y)
        10 + foo(x=x.y)
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_call() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                def x() -> int:
                    return 1
                def y() -> int:
                    return 2
            val = MyClass()

            foo(val.x())
            foo(val.y())",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
            def x() -> int:
                return 1
            def y() -> int:
                return 2
        val = MyClass()

        foo(val.x())
        foo([x=]val.y())
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
          |
        info: Source
          --> main2.py:12:6
           |
        12 | foo([x=]val.y())
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        9  | val = MyClass()
        10 |
        11 | foo(val.x())
           - foo(val.y())
        12 + foo(x=val.y())
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_complex() {
        let mut test = inlay_hint_test(
            "
            from typing import List

            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                def x() -> List[int]:
                    return 1
                def y() -> List[int]:
                    return 2
            val = MyClass()

            foo(val.x()[0])
            foo(val.y()[1])",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import List

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
            def x() -> List[int]:
                return 1
            def y() -> List[int]:
                return 2
        val = MyClass()

        foo(val.x()[0])
        foo([x=]val.y()[1])
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:9
          |
        4 | def foo(x: int): pass
          |         ^
          |
        info: Source
          --> main2.py:14:6
           |
        14 | foo([x=]val.y()[1])
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        11 | val = MyClass()
        12 |
        13 | foo(val.x()[0])
           - foo(val.y()[1])
        14 + foo(x=val.y()[1])
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_subscript() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int): pass
            x = [1]
            y = [2]

            foo(x[0])
            foo(y[0])",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int): pass
        x[: list[int]] = [1]
        y[: list[int]] = [2]

        foo(x[0])
        foo([x=]y[0])
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:3:5
          |
        3 | x[: list[int]] = [1]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:3:10
          |
        3 | x[: list[int]] = [1]
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:5
          |
        4 | y[: list[int]] = [2]
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:10
          |
        4 | y[: list[int]] = [2]
          |          ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
          |
        info: Source
         --> main2.py:7:6
          |
        7 | foo([x=]y[0])
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int): pass
          - x = [1]
          - y = [2]
        3 + x: list[int] = [1]
        4 + y: list[int] = [2]
        5 |
        6 | foo(x[0])
          - foo(y[0])
        7 + foo(x=y[0])
        ");
    }

    #[test]
    fn test_function_call_with_positional_only_parameter() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int, /): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, /): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_with_variadic_parameter() {
        let mut test = inlay_hint_test(
            "
            def foo(*args: int): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(*args: int): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_with_keyword_variadic_parameter() {
        let mut test = inlay_hint_test(
            "
            def foo(**kwargs: int): pass
            foo(x=1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(**kwargs: int): pass
        foo(x=1)
        ");
    }

    #[test]
    fn test_function_call_with_keyword_only_parameter() {
        let mut test = inlay_hint_test(
            "
            def foo(*, x: int): pass
            foo(x=1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(*, x: int): pass
        foo(x=1)
        ");
    }

    #[test]
    fn test_function_call_with_unpacked_tuple_argument() {
        // When an unpacked tuple fills multiple parameters, no hint should be shown
        // for that argument because showing a single parameter name would be misleading.
        let mut test = inlay_hint_test(
            "
            def foo(a: str, b: int, c: int, d: str): ...
            t: tuple[int, int] = (23, 42)
            foo('foo', *t, d='bar')",
        );

        // `*t` fills both `b` and `c`, so no hint is shown for it
        assert_snapshot!(test.inlay_hints(), @"

        def foo(a: str, b: int, c: int, d: str): ...
        t: tuple[int, int] = (23, 42)
        foo([a=]'foo', *t, d='bar')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(a: str, b: int, c: int, d: str): ...
          |         ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | foo([a=]'foo', *t, d='bar')
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(a: str, b: int, c: int, d: str): ...
        3 | t: tuple[int, int] = (23, 42)
          - foo('foo', *t, d='bar')
        4 + foo(a='foo', *t, d='bar')
        ");
    }

    #[test]
    fn test_function_call_with_unpacked_tuple_argument_single_element() {
        // When an unpacked tuple fills only one parameter, a hint should be shown.
        let mut test = inlay_hint_test(
            "
            def foo(a: str, b: int, c: str): ...
            t: tuple[int] = (42,)
            foo('foo', *t, 'bar')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(a: str, b: int, c: str): ...
        t: tuple[int] = (42,)
        foo([a=]'foo', [b=]*t, [c=]'bar')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(a: str, b: int, c: str): ...
          |         ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | foo([a=]'foo', [b=]*t, [c=]'bar')
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(a: str, b: int, c: str): ...
          |                 ^
          |
        info: Source
         --> main2.py:4:17
          |
        4 | foo([a=]'foo', [b=]*t, [c=]'bar')
          |                 ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:25
          |
        2 | def foo(a: str, b: int, c: str): ...
          |                         ^
          |
        info: Source
         --> main2.py:4:25
          |
        4 | foo([a=]'foo', [b=]*t, [c=]'bar')
          |                         ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(a: str, b: int, c: str): ...
        3 | t: tuple[int] = (42,)
          - foo('foo', *t, 'bar')
        4 + foo('foo', *t, c='bar')
        ");
    }

    #[test]
    fn test_function_call_last_plain_positional_before_starred_argument() {
        let mut test = inlay_hint_test(
            "
            def foo(a: int, b: int): ...
            t: tuple[int] = (2,)
            foo(1, *t)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(a: int, b: int): ...
        t: tuple[int] = (2,)
        foo([a=]1, [b=]*t)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(a: int, b: int): ...
          |         ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | foo([a=]1, [b=]*t)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(a: int, b: int): ...
          |                 ^
          |
        info: Source
         --> main2.py:4:13
          |
        4 | foo([a=]1, [b=]*t)
          |             ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(a: int, b: int): ...
        3 | t: tuple[int] = (2,)
          - foo(1, *t)
        4 + foo(a=1, *t)
        ");
    }

    #[test]
    fn test_function_call_only_starred_argument_has_no_edit() {
        let mut test = inlay_hint_test(
            "
            def foo(a: int): ...
            t: tuple[int] = (1,)
            foo(*t)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(a: int): ...
        t: tuple[int] = (1,)
        foo([a=]*t)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(a: int): ...
          |         ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | foo([a=]*t)
          |      ^
          |
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_positional_or_keyword_parameters() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int, /, y: int): pass
            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, /, y: int): pass
        foo(1, [y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:20
          |
        2 | def foo(x: int, /, y: int): pass
          |                    ^
          |
        info: Source
         --> main2.py:3:9
          |
        3 | foo(1, [y=]2)
          |         ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int, /, y: int): pass
          - foo(1, 2)
        3 + foo(1, y=2)
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_variadic_parameters() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int, /, *args: int): pass
            foo(1, 2, 3)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, /, *args: int): pass
        foo(1, 2, 3)
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_keyword_variadic_parameters() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int, /, **kwargs: int): pass
            foo(1, x=2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, /, **kwargs: int): pass
        foo(1, x=2)
        ");
    }

    #[test]
    fn test_class_constructor_call_init() {
        let mut test = inlay_hint_test(
            "
            class Foo:
                def __init__(self, x: int): pass
            Foo(1)
            f = Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class Foo:
            def __init__(self, x: int): pass
        Foo([x=]1)
        f = Foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: int): pass
          |                        ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | Foo([x=]1)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __init__(self, x: int): pass
          |                        ^
          |
        info: Source
         --> main2.py:5:10
          |
        5 | f = Foo([x=]1)
          |          ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | class Foo:
        3 |     def __init__(self, x: int): pass
          - Foo(1)
          - f = Foo(1)
        4 + Foo(x=1)
        5 + f = Foo(x=1)
        ");
    }

    #[test]
    fn test_named_tuple_constructor_call() {
        let mut test = inlay_hint_test(
            "
            from typing import NamedTuple

            class Foo(NamedTuple):
                x: int
                y: str

            Foo(1, 'a')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import NamedTuple

        class Foo(NamedTuple):
            x: int
            y: str

        Foo([x=]1, [y=]'a')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:5
          |
        5 |     x: int
          |     ^
          |
        info: Source
         --> main2.py:8:6
          |
        8 | Foo([x=]1, [y=]'a')
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:6:5
          |
        6 |     y: str
          |     ^
          |
        info: Source
         --> main2.py:8:13
          |
        8 | Foo([x=]1, [y=]'a')
          |             ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        5 |     x: int
        6 |     y: str
        7 |
          - Foo(1, 'a')
        8 + Foo(1, y='a')
        ");
    }

    #[test]
    fn test_class_constructor_call_new() {
        let mut test = inlay_hint_test(
            "
            class Foo:
                def __new__(cls, x: int): pass
            Foo(1)
            f = Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class Foo:
            def __new__(cls, x: int): pass
        Foo([x=]1)
        f = Foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:22
          |
        3 |     def __new__(cls, x: int): pass
          |                      ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | Foo([x=]1)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:22
          |
        3 |     def __new__(cls, x: int): pass
          |                      ^
          |
        info: Source
         --> main2.py:5:10
          |
        5 | f = Foo([x=]1)
          |          ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | class Foo:
        3 |     def __new__(cls, x: int): pass
          - Foo(1)
          - f = Foo(1)
        4 + Foo(x=1)
        5 + f = Foo(x=1)
        ");
    }

    #[test]
    fn test_class_constructor_call_meta_class_call() {
        let mut test = inlay_hint_test(
            "
            class MetaFoo:
                def __call__(self, x: int): pass
            class Foo(metaclass=MetaFoo):
                pass
            Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class MetaFoo:
            def __call__(self, x: int): pass
        class Foo(metaclass=MetaFoo):
            pass
        Foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        3 |     def __call__(self, x: int): pass
          |                        ^
          |
        info: Source
         --> main2.py:6:6
          |
        6 | Foo([x=]1)
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        3 |     def __call__(self, x: int): pass
        4 | class Foo(metaclass=MetaFoo):
        5 |     pass
          - Foo(1)
        6 + Foo(x=1)
        ");
    }

    #[test]
    fn test_callable_call() {
        let mut test = inlay_hint_test(
            "
            from typing import Callable
            def foo(x: Callable[[int], int]):
                x(1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import Callable
        def foo(x: Callable[[int], int]):
            x(1)
        ");
    }

    #[test]
    fn test_instance_method_call() {
        let mut test = inlay_hint_test(
            "
            class Foo:
                def bar(self, y: int): pass
            Foo().bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class Foo:
            def bar(self, y: int): pass
        Foo().bar([y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:19
          |
        3 |     def bar(self, y: int): pass
          |                   ^
          |
        info: Source
         --> main2.py:4:12
          |
        4 | Foo().bar([y=]2)
          |            ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | class Foo:
        3 |     def bar(self, y: int): pass
          - Foo().bar(2)
        4 + Foo().bar(y=2)
        ");
    }

    #[test]
    fn test_class_method_call() {
        let mut test = inlay_hint_test(
            "
            class Foo:
                @classmethod
                def bar(cls, y: int): pass
            Foo.bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class Foo:
            @classmethod
            def bar(cls, y: int): pass
        Foo.bar([y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:18
          |
        4 |     def bar(cls, y: int): pass
          |                  ^
          |
        info: Source
         --> main2.py:5:10
          |
        5 | Foo.bar([y=]2)
          |          ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        2 | class Foo:
        3 |     @classmethod
        4 |     def bar(cls, y: int): pass
          - Foo.bar(2)
        5 + Foo.bar(y=2)
        ");
    }

    #[test]
    fn test_static_method_call() {
        let mut test = inlay_hint_test(
            "
            class Foo:
                @staticmethod
                def bar(y: int): pass
            Foo.bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class Foo:
            @staticmethod
            def bar(y: int): pass
        Foo.bar([y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:13
          |
        4 |     def bar(y: int): pass
          |             ^
          |
        info: Source
         --> main2.py:5:10
          |
        5 | Foo.bar([y=]2)
          |          ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        2 | class Foo:
        3 |     @staticmethod
        4 |     def bar(y: int): pass
          - Foo.bar(2)
        5 + Foo.bar(y=2)
        ");
    }

    #[test]
    fn test_function_call_with_union_type() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int | str): pass
            foo(1)
            foo('abc')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int | str): pass
        foo([x=]1)
        foo([x=]'abc')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int | str): pass
          |         ^
          |
        info: Source
         --> main2.py:3:6
          |
        3 | foo([x=]1)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int | str): pass
          |         ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | foo([x=]'abc')
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int | str): pass
          - foo(1)
          - foo('abc')
        3 + foo(x=1)
        4 + foo(x='abc')
        ");
    }

    #[test]
    fn test_function_call_multiple_positional_arguments() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int, y: str, z: bool): pass
            foo(1, 'hello', True)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, y: str, z: bool): pass
        foo([x=]1, [y=]'hello', [z=]True)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |         ^
          |
        info: Source
         --> main2.py:3:6
          |
        3 | foo([x=]1, [y=]'hello', [z=]True)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |                 ^
          |
        info: Source
         --> main2.py:3:13
          |
        3 | foo([x=]1, [y=]'hello', [z=]True)
          |             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:25
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |                         ^
          |
        info: Source
         --> main2.py:3:26
          |
        3 | foo([x=]1, [y=]'hello', [z=]True)
          |                          ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int, y: str, z: bool): pass
          - foo(1, 'hello', True)
        3 + foo(1, 'hello', z=True)
        ");
    }

    #[test]
    fn test_function_call_multiple_positional_arguments_before_keyword() {
        let mut test = inlay_hint_test(
            "
            def add(x: int, b, y: int) -> int:
                return x + y

            total = add(3, 2, y=4)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def add(x: int, b, y: int) -> int:
            return x + y

        total[: int] = add([x=]3, [b=]2, y=4)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:5:9
          |
        5 | total[: int] = add([x=]3, [b=]2, y=4)
          |         ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def add(x: int, b, y: int) -> int:
          |         ^
          |
        info: Source
         --> main2.py:5:21
          |
        5 | total[: int] = add([x=]3, [b=]2, y=4)
          |                     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def add(x: int, b, y: int) -> int:
          |                 ^
          |
        info: Source
         --> main2.py:5:28
          |
        5 | total[: int] = add([x=]3, [b=]2, y=4)
          |                            ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        2 | def add(x: int, b, y: int) -> int:
        3 |     return x + y
        4 |
          - total = add(3, 2, y=4)
        5 + total: int = add(3, b=2, y=4)
        ");
    }

    #[test]
    fn test_function_call_mixed_positional_and_keyword() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int, y: str, z: bool): pass
            foo(1, z=True, y='hello')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, y: str, z: bool): pass
        foo([x=]1, z=True, y='hello')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |         ^
          |
        info: Source
         --> main2.py:3:6
          |
        3 | foo([x=]1, z=True, y='hello')
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int, y: str, z: bool): pass
          - foo(1, z=True, y='hello')
        3 + foo(x=1, z=True, y='hello')
        ");
    }

    #[test]
    fn test_function_call_positional_after_keyword_in_source_order() {
        // ty should continue to map positional args correctly in invalid or in-progress code,
        // even if a keyword arg appears earlier in source order.
        let mut test = inlay_hint_test(
            "
            def foo(x: int, y: str): pass
            foo(y='hello', 1)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, y: str): pass
        foo(y='hello', [y=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str): pass
          |                 ^
          |
        info: Source
         --> main2.py:3:17
          |
        3 | foo(y='hello', [y=]1)
          |                 ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int, y: str): pass
          - foo(y='hello', 1)
        3 + foo(y='hello', y=1)
        ");
    }

    #[test]
    fn test_function_call_with_default_parameters() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int, y: str = 'default', z: bool = False): pass
            foo(1)
            foo(1, 'custom')
            foo(1, 'custom', True)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, y: str = 'default', z: bool = False): pass
        foo([x=]1)
        foo([x=]1, [y=]'custom')
        foo([x=]1, [y=]'custom', [z=]True)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |         ^
          |
        info: Source
         --> main2.py:3:6
          |
        3 | foo([x=]1)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |         ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | foo([x=]1, [y=]'custom')
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |                 ^
          |
        info: Source
         --> main2.py:4:13
          |
        4 | foo([x=]1, [y=]'custom')
          |             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |         ^
          |
        info: Source
         --> main2.py:5:6
          |
        5 | foo([x=]1, [y=]'custom', [z=]True)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |                 ^
          |
        info: Source
         --> main2.py:5:13
          |
        5 | foo([x=]1, [y=]'custom', [z=]True)
          |             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:37
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |                                     ^
          |
        info: Source
         --> main2.py:5:27
          |
        5 | foo([x=]1, [y=]'custom', [z=]True)
          |                           ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          - foo(1)
          - foo(1, 'custom')
          - foo(1, 'custom', True)
        3 + foo(x=1)
        4 + foo(1, y='custom')
        5 + foo(1, 'custom', z=True)
        ");
    }

    #[test]
    fn test_nested_function_calls() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int) -> int:
                return x * 2

            def bar(y: str) -> str:
                return y

            def baz(a: int, b: str, c: bool): pass

            baz(foo(5), bar(bar('test')), True)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int) -> int:
            return x * 2

        def bar(y: str) -> str:
            return y

        def baz(a: int, b: str, c: bool): pass

        baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:8:9
          |
        8 | def baz(a: int, b: str, c: bool): pass
          |         ^
          |
        info: Source
          --> main2.py:10:6
           |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |      ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int) -> int:
          |         ^
          |
        info: Source
          --> main2.py:10:14
           |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |              ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:8:17
          |
        8 | def baz(a: int, b: str, c: bool): pass
          |                 ^
          |
        info: Source
          --> main2.py:10:22
           |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                      ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:9
          |
        5 | def bar(y: str) -> str:
          |         ^
          |
        info: Source
          --> main2.py:10:30
           |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                              ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:9
          |
        5 | def bar(y: str) -> str:
          |         ^
          |
        info: Source
          --> main2.py:10:38
           |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                                      ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:8:25
          |
        8 | def baz(a: int, b: str, c: bool): pass
          |                         ^
          |
        info: Source
          --> main2.py:10:52
           |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                                                    ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        7  |
        8  | def baz(a: int, b: str, c: bool): pass
        9  |
           - baz(foo(5), bar(bar('test')), True)
        10 + baz(foo(x=5), bar(y=bar(y='test')), c=True)
        ");
    }

    #[test]
    fn test_method_chaining() {
        let mut test = inlay_hint_test(
            "
            class A:
                def foo(self, value: int) -> 'A':
                    return self
                def bar(self, name: str) -> 'A':
                    return self
                def baz(self): pass
            A().foo(42).bar('test').baz()",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class A:
            def foo(self, value: int) -> 'A':
                return self
            def bar(self, name: str) -> 'A':
                return self
            def baz(self): pass
        A().foo([value=]42).bar([name=]'test').baz()
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:19
          |
        3 |     def foo(self, value: int) -> 'A':
          |                   ^^^^^
          |
        info: Source
         --> main2.py:8:10
          |
        8 | A().foo([value=]42).bar([name=]'test').baz()
          |          ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:19
          |
        5 |     def bar(self, name: str) -> 'A':
          |                   ^^^^
          |
        info: Source
         --> main2.py:8:26
          |
        8 | A().foo([value=]42).bar([name=]'test').baz()
          |                          ^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        5 |     def bar(self, name: str) -> 'A':
        6 |         return self
        7 |     def baz(self): pass
          - A().foo(42).bar('test').baz()
        8 + A().foo(value=42).bar(name='test').baz()
        ");
    }

    #[test]
    fn test_nested_keyword_function_calls() {
        let mut test = inlay_hint_test(
            "
            def foo(x: str) -> str:
                return x
            def bar(y: int): pass
            bar(y=foo('test'))
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: str) -> str:
            return x
        def bar(y: int): pass
        bar(y=foo([x=]'test'))

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: str) -> str:
          |         ^
          |
        info: Source
         --> main2.py:5:12
          |
        5 | bar(y=foo([x=]'test'))
          |            ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        2 | def foo(x: str) -> str:
        3 |     return x
        4 | def bar(y: int): pass
          - bar(y=foo('test'))
        5 + bar(y=foo(x='test'))
        ");
    }

    #[test]
    fn test_lambda_function_calls() {
        let mut test = inlay_hint_test(
            "
            foo = lambda x: x * 2
            bar = lambda a, b: a + b
            foo(5)
            bar(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        foo[: (x) -> Unknown] = lambda x: x * 2
        bar[: (a, b) -> Unknown] = lambda a, b: a + b
        foo([x=]5)
        bar([a=]1, [b=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:14:1
           |
        14 | Unknown: _SpecialForm
           | ^^^^^^^
           |
        info: Source
         --> main2.py:2:14
          |
        2 | foo[: (x) -> Unknown] = lambda x: x * 2
          |              ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:14:1
           |
        14 | Unknown: _SpecialForm
           | ^^^^^^^
           |
        info: Source
         --> main2.py:3:17
          |
        3 | bar[: (a, b) -> Unknown] = lambda a, b: a + b
          |                 ^^^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | foo = lambda x: x * 2
        3 | bar = lambda a, b: a + b
          - foo(5)
          - bar(1, 2)
        4 + foo(x=5)
        5 + bar(1, b=2)
        ");
    }

    #[test]
    fn test_literal_string() {
        let mut test = inlay_hint_test(
            r#"
            from typing import LiteralString
            def my_func(x: LiteralString):
                y = x
            my_func(x="hello")"#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        from typing import LiteralString
        def my_func(x: LiteralString):
            y[: LiteralString] = x
        my_func(x="hello")
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:9
          |
        4 |     y[: LiteralString] = x
          |         ^^^^^^^^^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | from typing import LiteralString
        3 | def my_func(x: LiteralString):
          -     y = x
        4 +     y: LiteralString = x
        5 | my_func(x="hello")
        "#);
    }

    #[test]
    fn test_literal_group() {
        let mut test = inlay_hint_test(
            r#"
            def branch(cond: int):
                if cond < 10:
                    x = 1
                elif cond < 20:
                    x = 2
                elif cond < 30:
                    x = 3
                elif cond < 40:
                    x = "hello"
                else:
                    x = None
                y = x"#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def branch(cond: int):
            if cond < 10:
                x = 1
            elif cond < 20:
                x = 2
            elif cond < 30:
                x = 3
            elif cond < 40:
                x = "hello"
            else:
                x = None
            y[: Literal[1, 2, 3, "hello"] | None] = x
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
          --> main2.py:13:9
           |
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |         ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:13:17
           |
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                 ^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:13:20
           |
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                    ^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
          --> main2.py:13:23
           |
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                       ^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:13:26
           |
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                          ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:969:11
            |
        969 |     class NoneType:
            |           ^^^^^^^^
            |
        info: Source
          --> main2.py:13:37
           |
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                                     ^^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1  + from typing import Literal
        2  |
        3  | def branch(cond: int):
        4  |     if cond < 10:
        --------------------------------------------------------------------------------
        11 |         x = "hello"
        12 |     else:
        13 |         x = None
           -     y = x
        14 +     y: Literal[1, 2, 3, "hello"] | None = x
        "#);
    }

    #[test]
    fn test_generic_alias() {
        let mut test = inlay_hint_test(
            r"
            class Foo[T]: ...

            a = Foo[int]",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class Foo[T]: ...

        a[: <class 'Foo[int]'>] = Foo[int]
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class Foo[T]: ...
          |       ^^^
          |
        info: Source
         --> main2.py:4:13
          |
        4 | a[: <class 'Foo[int]'>] = Foo[int]
          |             ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:17
          |
        4 | a[: <class 'Foo[int]'>] = Foo[int]
          |                 ^^^
          |
        ");
    }

    #[test]
    fn test_subclass_type() {
        let mut test = inlay_hint_test(
            r"
            def f(x: list[str]):
                y = type(x)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def f(x: list[str]):
            y[: type[list[str]]] = type(x)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:247:7
            |
        247 | class type:
            |       ^^^^
            |
        info: Source
         --> main2.py:3:9
          |
        3 |     y[: type[list[str]]] = type(x)
          |         ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:3:14
          |
        3 |     y[: type[list[str]]] = type(x)
          |              ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:3:19
          |
        3 |     y[: type[list[str]]] = type(x)
          |                   ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def f(x: list[str]):
          -     y = type(x)
        3 +     y: type[list[str]] = type(x)
        ");
    }

    #[test]
    fn test_property_literal_type() {
        let mut test = inlay_hint_test(
            r"
            class F:
                @property
                def whatever(self): ...

            ab = F.whatever",
        );

        assert_snapshot!(test.inlay_hints(), @"

        class F:
            @property
            def whatever(self): ...

        ab[: property] = F.whatever
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:9
          |
        4 |     def whatever(self): ...
          |         ^^^^^^^^
          |
        info: Source
         --> main2.py:6:6
          |
        6 | ab[: property] = F.whatever
          |      ^^^^^^^^
          |
        ");
    }

    #[test]
    fn test_complex_parameter_combinations() {
        let mut test = inlay_hint_test(
            "
            def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
            foo(1, 'pos', 3.14, False, e=42)
            foo(1, 'pos', 3.14, e=42, f='custom')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        foo(1, 'pos', [c=]3.14, [d=]False, e=42)
        foo(1, 'pos', [c=]3.14, e=42, f='custom')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:28
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
          |                            ^
          |
        info: Source
         --> main2.py:3:16
          |
        3 | foo(1, 'pos', [c=]3.14, [d=]False, e=42)
          |                ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:38
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
          |                                      ^
          |
        info: Source
         --> main2.py:3:26
          |
        3 | foo(1, 'pos', [c=]3.14, [d=]False, e=42)
          |                          ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:28
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
          |                            ^
          |
        info: Source
         --> main2.py:4:16
          |
        4 | foo(1, 'pos', [c=]3.14, e=42, f='custom')
          |                ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
          - foo(1, 'pos', 3.14, False, e=42)
          - foo(1, 'pos', 3.14, e=42, f='custom')
        3 + foo(1, 'pos', 3.14, d=False, e=42)
        4 + foo(1, 'pos', c=3.14, e=42, f='custom')
        ");
    }

    #[test]
    fn test_function_calls_different_file() {
        let mut test = inlay_hint_test(
            "
            from foo import bar

            bar(1)",
        );

        test.with_extra_file(
            "foo.py",
            "
        def bar(x: int | str):
            pass",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from foo import bar

        bar([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:2:17
          |
        2 |         def bar(x: int | str):
          |                 ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | bar([x=]1)
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | from foo import bar
        3 |
          - bar(1)
        4 + bar(x=1)
        ");
    }

    #[test]
    fn test_overloaded_function_calls() {
        let mut test = inlay_hint_test(
            "
            from typing import overload

            @overload
            def foo(x: int) -> str: ...
            @overload
            def foo(x: str) -> int: ...
            def foo(x):
                return x

            foo(42)
            foo('hello')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import overload

        @overload
        def foo(x: int) -> str: ...
        @overload
        def foo(x: str) -> int: ...
        def foo(x):
            return x

        foo([x=]42)
        foo([x=]'hello')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:9
          |
        5 | def foo(x: int) -> str: ...
          |         ^
          |
        info: Source
          --> main2.py:11:6
           |
        11 | foo([x=]42)
           |      ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:7:9
          |
        7 | def foo(x: str) -> int: ...
          |         ^
          |
        info: Source
          --> main2.py:12:6
           |
        12 | foo([x=]'hello')
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        8  | def foo(x):
        9  |     return x
        10 |
           - foo(42)
           - foo('hello')
        11 + foo(x=42)
        12 + foo(x='hello')
        ");
    }

    #[test]
    fn test_overloaded_function_calls_different_params() {
        let mut test = inlay_hint_test(
            "
            from typing import overload, Optional, Sequence

            @overload
            def S(name: str, is_symmetric: Optional[bool] = None) -> str: ...
            @overload
            def S(*names: str, is_symmetric: Optional[bool] = None) -> Sequence[str]: ...
            def S():
                pass

            b = S('x', 'y')",
        );

        // The call S('x', 'y') should match the second overload (*names: str),
        // and since *names is variadic, no parameter name hints should be shown.
        // Before the fix, this incorrectly showed `name=` and `is_symmetric=` hints
        // from the first overload.
        assert_snapshot!(test.inlay_hints(), @"

        from typing import overload, Optional, Sequence

        @overload
        def S(name: str, is_symmetric: Optional[bool] = None) -> str: ...
        @overload
        def S(*names: str, is_symmetric: Optional[bool] = None) -> Sequence[str]: ...
        def S():
            pass

        b[: Sequence[str]] = S('x', 'y')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/typing.pyi:1565:7
             |
        1565 | class Sequence(Reversible[_T_co], Collection[_T_co]):
             |       ^^^^^^^^
             |
        info: Source
          --> main2.py:11:5
           |
        11 | b[: Sequence[str]] = S('x', 'y')
           |     ^^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
          --> main2.py:11:14
           |
        11 | b[: Sequence[str]] = S('x', 'y')
           |              ^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        8  | def S():
        9  |     pass
        10 |
           - b = S('x', 'y')
        11 + b: Sequence[str] = S('x', 'y')
        ");
    }

    #[test]
    fn test_overloaded_function_calls_no_matching_overload() {
        let mut test = inlay_hint_test(
            "
            from typing import overload

            @overload
            def f(x: int) -> str: ...
            @overload
            def f(x: str, y: str) -> int: ...
            def f(x):
                return x

            f([])
            ",
        );

        // Neither overload matches via type checking (list[Unknown] is neither int nor str),
        // so `matching_overloads()` returns empty. The arity-based fallback picks the first
        // overload (1 matched arg out of 1 required), and we should see the `x=` hint.
        assert_snapshot!(test.inlay_hints(), @"

        from typing import overload

        @overload
        def f(x: int) -> str: ...
        @overload
        def f(x: str, y: str) -> int: ...
        def f(x):
            return x

        f([x=][])

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:7
          |
        5 | def f(x: int) -> str: ...
          |       ^
          |
        info: Source
          --> main2.py:11:4
           |
        11 | f([x=][])
           |    ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        8  | def f(x):
        9  |     return x
        10 |
           - f([])
        11 + f(x=[])
        ");
    }

    #[test]
    fn test_disabled_function_argument_names() {
        let mut test = inlay_hint_test(
            "
        def foo(x: int): pass
        foo(1)",
        );

        assert_snapshot!(test.inlay_hints_with_settings(&InlayHintSettings {
            call_argument_names: false,
            ..Default::default()
        }), @"

        def foo(x: int): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_argument_name_suppressed_by_case_insensitive_exact_match() {
        let mut test = inlay_hint_test(
            "
            def foo(test: int, param: int): pass
            TEST = 1
            PARAM = 1

            foo(TEST, PARAM)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(test: int, param: int): pass
        TEST = 1
        PARAM = 1

        foo(TEST, PARAM)
        ");
    }

    #[test]
    fn test_function_call_argument_name_suppressed_by_normalized_parameter_name() {
        let mut test = inlay_hint_test(
            "
            def trailing(param_: int): pass
            def leading(_param: int): pass
            param = 1

            trailing(param)
            leading(param)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def trailing(param_: int): pass
        def leading(_param: int): pass
        param = 1

        trailing(param)
        leading(param)
        ");
    }

    #[test]
    fn test_function_call_argument_name_suppressed_by_segment_prefix_or_suffix() {
        let mut test = inlay_hint_test(
            "
            def foo(param: int): pass
            param = 1
            param_end = 1
            start_param = 1

            foo(param)
            foo(param_end)
            foo(start_param)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(param: int): pass
        param = 1
        param_end = 1
        start_param = 1

        foo(param)
        foo(param_end)
        foo(start_param)
        ");
    }

    #[test]
    fn test_function_call_argument_name_shown_for_near_matches() {
        let mut test = inlay_hint_test(
            "
            def foo(param: int): pass
            param2 = 1
            my_param2 = 1
            parameter = 1

            foo(param2)
            foo(my_param2)
            foo(parameter)",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def foo(param: int): pass
        param2 = 1
        my_param2 = 1
        parameter = 1

        foo([param=]param2)
        foo([param=]my_param2)
        foo([param=]parameter)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(param: int): pass
          |         ^^^^^
          |
        info: Source
         --> main2.py:7:6
          |
        7 | foo([param=]param2)
          |      ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(param: int): pass
          |         ^^^^^
          |
        info: Source
         --> main2.py:8:6
          |
        8 | foo([param=]my_param2)
          |      ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(param: int): pass
          |         ^^^^^
          |
        info: Source
         --> main2.py:9:6
          |
        9 | foo([param=]parameter)
          |      ^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        4 | my_param2 = 1
        5 | parameter = 1
        6 |
          - foo(param2)
          - foo(my_param2)
          - foo(parameter)
        7 + foo(param=param2)
        8 + foo(param=my_param2)
        9 + foo(param=parameter)
        "#);
    }

    #[test]
    fn test_function_call_argument_name_suppression_matches_full_segment_sequence() {
        let mut test = inlay_hint_test(
            "
            def foo(focus_range: int): pass
            focus_range = 1
            FOCUS_RANGE = 1
            focus_range_end = 1
            start_focus_range = 1
            focus_end_range = 1

            foo(focus_range)
            foo(FOCUS_RANGE)
            foo(focus_range_end)
            foo(start_focus_range)
            foo(focus_end_range)",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        def foo(focus_range: int): pass
        focus_range = 1
        FOCUS_RANGE = 1
        focus_range_end = 1
        start_focus_range = 1
        focus_end_range = 1

        foo(focus_range)
        foo(FOCUS_RANGE)
        foo(focus_range_end)
        foo(start_focus_range)
        foo([focus_range=]focus_end_range)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(focus_range: int): pass
          |         ^^^^^^^^^^^
          |
        info: Source
          --> main2.py:13:6
           |
        13 | foo([focus_range=]focus_end_range)
           |      ^^^^^^^^^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        10 | foo(FOCUS_RANGE)
        11 | foo(focus_range_end)
        12 | foo(start_focus_range)
           - foo(focus_end_range)
        13 + foo(focus_range=focus_end_range)
        "#);
    }

    #[test]
    fn test_function_call_out_of_range() {
        let mut test = inlay_hint_test(
            "
            <START>def foo(x: int): pass
            def bar(y: int): pass
            foo(1)<END>
            bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int): pass
        def bar(y: int): pass
        foo([x=]1)
        bar(2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
          |
        info: Source
         --> main2.py:4:6
          |
        4 | foo([x=]1)
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(x: int): pass
        3 | def bar(y: int): pass
          - foo(1)
        4 + foo(x=1)
        5 | bar(2)
        ");
    }

    #[test]
    fn test_function_call_with_argument_name_starting_with_underscore() {
        let mut test = inlay_hint_test(
            "
            def foo(_x: int, y: int): pass
            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(_x: int, y: int): pass
        foo(1, [y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:18
          |
        2 | def foo(_x: int, y: int): pass
          |                  ^
          |
        info: Source
         --> main2.py:3:9
          |
        3 | foo(1, [y=]2)
          |         ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | def foo(_x: int, y: int): pass
          - foo(1, 2)
        3 + foo(1, y=2)
        ");
    }

    #[test]
    fn test_function_call_different_formatting() {
        let mut test = inlay_hint_test(
            "
            def foo(
                x: int,
                y: int
            ): ...

            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(
            x: int,
            y: int
        ): ...

        foo([x=]1, [y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:5
          |
        3 |     x: int,
          |     ^
          |
        info: Source
         --> main2.py:7:6
          |
        7 | foo([x=]1, [y=]2)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:5
          |
        4 |     y: int
          |     ^
          |
        info: Source
         --> main2.py:7:13
          |
        7 | foo([x=]1, [y=]2)
          |             ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        4 |     y: int
        5 | ): ...
        6 |
          - foo(1, 2)
        7 + foo(1, y=2)
        ");
    }

    #[test]
    fn test_function_signature_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        def foo(x: int, *y: bool, z: str | int | list[str]): ...

        a = foo",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def foo(x: int, *y: bool, z: str | int | list[str]): ...

        a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:16
          |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2618:7
             |
        2618 | class bool(int):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:25
          |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                         ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:37
          |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:43
          |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                           ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:49
          |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                                 ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:54
          |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                                      ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:14:1
           |
        14 | Unknown: _SpecialForm
           | ^^^^^^^
           |
        info: Source
         --> main2.py:4:63
          |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                                               ^^^^^^^
          |
        ");
    }

    #[test]
    fn test_module_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        import foo

        a = foo",
        );

        test.with_extra_file("foo.py", "'''Foo module'''");

        assert_snapshot!(test.inlay_hints(), @"

        import foo

        a[: <module 'foo'>] = foo
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:431:7
            |
        431 | class ModuleType:
            |       ^^^^^^^^^^
            |
        info: Source
         --> main2.py:4:6
          |
        4 | a[: <module 'foo'>] = foo
          |      ^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:1:1
          |
        1 | '''Foo module'''
          | ^^^^^^^^^^^^^^^^
          |
        info: Source
         --> main2.py:4:14
          |
        4 | a[: <module 'foo'>] = foo
          |              ^^^
          |
        ");
    }

    #[test]
    fn test_literal_type_alias_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        from typing import Literal

        a = Literal['a', 'b', 'c']",
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        from typing import Literal

        a[: <special-form 'Literal["a", "b", "c"]'>] = Literal['a', 'b', 'c']
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:4:20
          |
        4 | a[: <special-form 'Literal["a", "b", "c"]'>] = Literal['a', 'b', 'c']
          |                    ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:28
          |
        4 | a[: <special-form 'Literal["a", "b", "c"]'>] = Literal['a', 'b', 'c']
          |                            ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:33
          |
        4 | a[: <special-form 'Literal["a", "b", "c"]'>] = Literal['a', 'b', 'c']
          |                                 ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:38
          |
        4 | a[: <special-form 'Literal["a", "b", "c"]'>] = Literal['a', 'b', 'c']
          |                                      ^^^
          |
        "#);
    }

    #[test]
    fn test_wrapper_descriptor_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        from types import FunctionType

        a = FunctionType.__get__",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from types import FunctionType

        a[: <wrapper-descriptor '__get__' of 'function' objects>] = FunctionType.__get__
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:685:7
            |
        685 | class WrapperDescriptorType:
            |       ^^^^^^^^^^^^^^^^^^^^^
            |
        info: Source
         --> main2.py:4:6
          |
        4 | a[: <wrapper-descriptor '__get__' of 'function' objects>] = FunctionType.__get__
          |      ^^^^^^^^^^^^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/types.pyi:77:7
           |
        77 | class FunctionType:
           |       ^^^^^^^^^^^^
           |
        info: Source
         --> main2.py:4:39
          |
        4 | a[: <wrapper-descriptor '__get__' of 'function' objects>] = FunctionType.__get__
          |                                       ^^^^^^^^
          |
        ");
    }

    #[test]
    fn test_method_wrapper_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        def f(): ...

        a = f.__call__",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def f(): ...

        a[: <method-wrapper '__call__' of function 'f'>] = f.__call__
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:699:7
            |
        699 | class MethodWrapperType:
            |       ^^^^^^^^^^^^^^^^^
            |
        info: Source
         --> main2.py:4:6
          |
        4 | a[: <method-wrapper '__call__' of function 'f'>] = f.__call__
          |      ^^^^^^^^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:139:9
            |
        139 |     def __call__(self, *args: Any, **kwargs: Any) -> Any:
            |         ^^^^^^^^
            |
        info: Source
         --> main2.py:4:22
          |
        4 | a[: <method-wrapper '__call__' of function 'f'>] = f.__call__
          |                      ^^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/types.pyi:77:7
           |
        77 | class FunctionType:
           |       ^^^^^^^^^^^^
           |
        info: Source
         --> main2.py:4:35
          |
        4 | a[: <method-wrapper '__call__' of function 'f'>] = f.__call__
          |                                   ^^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:5
          |
        2 | def f(): ...
          |     ^
          |
        info: Source
         --> main2.py:4:45
          |
        4 | a[: <method-wrapper '__call__' of function 'f'>] = f.__call__
          |                                             ^
          |
        ");
    }

    #[test]
    fn test_newtype_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        from typing import NewType

        N = NewType('N', str)

        Y = N",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import NewType

        N[: <NewType pseudo-class 'N'>] = NewType([name=]'N', [tp=]str)

        Y[: <NewType pseudo-class 'N'>] = N
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/typing.pyi:1040:11
             |
        1040 |     class NewType:
             |           ^^^^^^^
             |
        info: Source
         --> main2.py:4:6
          |
        4 | N[: <NewType pseudo-class 'N'>] = NewType([name=]'N', [tp=]str)
          |      ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:1
          |
        4 | N = NewType('N', str)
          | ^
          |
        info: Source
         --> main2.py:4:28
          |
        4 | N[: <NewType pseudo-class 'N'>] = NewType([name=]'N', [tp=]str)
          |                            ^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/typing.pyi:1062:28
             |
        1062 |         def __init__(self, name: str, tp: Any) -> None: ...  # AnnotationForm
             |                            ^^^^
             |
        info: Source
         --> main2.py:4:44
          |
        4 | N[: <NewType pseudo-class 'N'>] = NewType([name=]'N', [tp=]str)
          |                                            ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/typing.pyi:1062:39
             |
        1062 |         def __init__(self, name: str, tp: Any) -> None: ...  # AnnotationForm
             |                                       ^^
             |
        info: Source
         --> main2.py:4:56
          |
        4 | N[: <NewType pseudo-class 'N'>] = NewType([name=]'N', [tp=]str)
          |                                                        ^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/typing.pyi:1040:11
             |
        1040 |     class NewType:
             |           ^^^^^^^
             |
        info: Source
         --> main2.py:6:6
          |
        6 | Y[: <NewType pseudo-class 'N'>] = N
          |      ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:1
          |
        4 | N = NewType('N', str)
          | ^
          |
        info: Source
         --> main2.py:6:28
          |
        6 | Y[: <NewType pseudo-class 'N'>] = N
          |                            ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | from typing import NewType
        3 |
          - N = NewType('N', str)
        4 + N = NewType('N', tp=str)
        5 |
        6 | Y = N
        ");
    }

    #[test]
    fn test_meta_typevar_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        def f[T](x: type[T]):
            y = x",
        );

        assert_snapshot!(test.inlay_hints(), @"

        def f[T](x: type[T]):
            y[: type[T@f]] = x
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:247:7
            |
        247 | class type:
            |       ^^^^
            |
        info: Source
         --> main2.py:3:9
          |
        3 |     y[: type[T@f]] = x
          |         ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | def f[T](x: type[T]):
          |       ^
          |
        info: Source
         --> main2.py:3:14
          |
        3 |     y[: type[T@f]] = x
          |              ^^^
          |
        ");
    }

    #[test]
    fn test_subscripted_protocol_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        from typing import Protocol, TypeVar
        T = TypeVar('T')
        Strange = Protocol[T]",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import Protocol, TypeVar
        T = TypeVar([name=]'T')
        Strange[: <special-form 'typing.Protocol[T]'>] = Protocol[T]
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:276:13
            |
        276 |             name: str,
            |             ^^^^
            |
        info: Source
         --> main2.py:3:14
          |
        3 | T = TypeVar([name=]'T')
          |              ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:346:1
            |
        346 | Protocol: _SpecialForm
            | ^^^^^^^^
            |
        info: Source
         --> main2.py:4:26
          |
        4 | Strange[: <special-form 'typing.Protocol[T]'>] = Protocol[T]
          |                          ^^^^^^^^^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:1
          |
        3 | T = TypeVar('T')
          | ^
          |
        info: Source
         --> main2.py:4:42
          |
        4 | Strange[: <special-form 'typing.Protocol[T]'>] = Protocol[T]
          |                                          ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | from typing import Protocol, TypeVar
          - T = TypeVar('T')
        3 + T = TypeVar(name='T')
        4 | Strange = Protocol[T]
        ");
    }

    #[test]
    fn test_paramspec_creation_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        from typing import ParamSpec
        P = ParamSpec('P')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing import ParamSpec
        P = ParamSpec([name=]'P')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:901:17
            |
        901 |                 name: str,
            |                 ^^^^
            |
        info: Source
         --> main2.py:3:16
          |
        3 | P = ParamSpec([name=]'P')
          |                ^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | from typing import ParamSpec
          - P = ParamSpec('P')
        3 + P = ParamSpec(name='P')
        ");
    }

    #[test]
    fn test_typealiastype_creation_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        from typing_extensions import TypeAliasType
        A = TypeAliasType('A', str)",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing_extensions import TypeAliasType
        A = TypeAliasType([name=]'A', [value=]str)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/typing.pyi:2561:26
             |
        2561 |         def __new__(cls, name: str, value: Any, *, type_params: tuple[_TypeParameter, ...] = ()) -> Self: ...
             |                          ^^^^
             |
        info: Source
         --> main2.py:3:20
          |
        3 | A = TypeAliasType([name=]'A', [value=]str)
          |                    ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/typing.pyi:2561:37
             |
        2561 |         def __new__(cls, name: str, value: Any, *, type_params: tuple[_TypeParameter, ...] = ()) -> Self: ...
             |                                     ^^^^^
             |
        info: Source
         --> main2.py:3:32
          |
        3 | A = TypeAliasType([name=]'A', [value=]str)
          |                                ^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | from typing_extensions import TypeAliasType
          - A = TypeAliasType('A', str)
        3 + A = TypeAliasType('A', value=str)
        ");
    }

    #[test]
    fn test_typevartuple_creation_inlay_hint() {
        let mut test = inlay_hint_test(
            "
        from typing_extensions import TypeVarTuple
        Ts = TypeVarTuple('Ts')",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from typing_extensions import TypeVarTuple
        Ts = TypeVarTuple([name=]'Ts')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:761:30
            |
        761 |             def __new__(cls, name: str, *, default: Any = ...) -> Self: ...  # AnnotationForm
            |                              ^^^^
            |
        info: Source
         --> main2.py:3:20
          |
        3 | Ts = TypeVarTuple([name=]'Ts')
          |                    ^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
        2 | from typing_extensions import TypeVarTuple
          - Ts = TypeVarTuple('Ts')
        3 + Ts = TypeVarTuple(name='Ts')
        ");
    }

    #[test]
    fn hover_narrowed_type_with_top_materialization() {
        let mut test = inlay_hint_test(
            r#"
                def f(xyxy: object):
                    if isinstance(xyxy, list):
                        x = xyxy
                "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        def f(xyxy: object):
            if isinstance(xyxy, list):
                x[: Top[list[Unknown]]] = xyxy

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:44:1
           |
        44 | Top: _SpecialForm
           | ^^^
           |
        info: Source
         --> main2.py:4:13
          |
        4 |         x[: Top[list[Unknown]]] = xyxy
          |             ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:17
          |
        4 |         x[: Top[list[Unknown]]] = xyxy
          |                 ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:14:1
           |
        14 | Unknown: _SpecialForm
           | ^^^^^^^
           |
        info: Source
         --> main2.py:4:22
          |
        4 |         x[: Top[list[Unknown]]] = xyxy
          |                      ^^^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + from ty_extensions import Top
        2 + from ty_extensions import Unknown
        3 |
        4 | def f(xyxy: object):
        5 |     if isinstance(xyxy, list):
          -         x = xyxy
        6 +         x: Top[list[Unknown]] = xyxy
        ");
    }

    #[test]
    fn test_auto_import_with_qualification_of_names() {
        let mut test = inlay_hint_test(
            "
            import foo

            a = foo.C().foo()
            ",
        );

        test.with_extra_file(
            "foo.py",
            "
            import bar

            class A[T]: ...

            class B[T]: ...

            class C:
                def foo(self) -> B[A[bar.D[int, list[str | A[B[int]]]]]]:
                    raise NotImplementedError
                    ",
        );

        test.with_extra_file(
            "bar.py",
            "
            class D[T, U]: ...
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        import foo

        a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:6:19
          |
        6 |             class B[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:5
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:4:19
          |
        4 |             class A[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:7
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |       ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> bar.py:2:19
          |
        2 |             class D[T, U]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:9
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |         ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:11
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |           ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:16
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |                ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:21
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |                     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:4:19
          |
        4 |             class A[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:27
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |                           ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:6:19
          |
        6 |             class B[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:29
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |                             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:31
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = foo.C().foo()
          |                               ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + from bar import D
        2 |
        3 | import foo
        4 |
          - a = foo.C().foo()
        5 + a: foo.B[foo.A[D[int, list[str | foo.A[foo.B[int]]]]]] = foo.C().foo()
        ");
    }

    #[test]
    fn test_auto_import_with_update_import_from_statement() {
        let mut test = inlay_hint_test(
            "
            from foo import C

            a = C().foo()
            ",
        );

        test.with_extra_file(
            "foo.py",
            "
            import bar

            class A[T]: ...

            class B[T]: ...

            class C:
                def foo(self) -> B[A[bar.D[int, list[str | A[B[int]]]]]]:
                    raise NotImplementedError
                    ",
        );

        test.with_extra_file(
            "bar.py",
            "
            class D[T, U]: ...
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from foo import C

        a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:6:19
          |
        6 |             class B[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:5
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:4:19
          |
        4 |             class A[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:7
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |       ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> bar.py:2:19
          |
        2 |             class D[T, U]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:9
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |         ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:11
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |           ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:16
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |                ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:4:21
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |                     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:4:19
          |
        4 |             class A[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:27
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |                           ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:6:19
          |
        6 |             class B[T]: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:29
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |                             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        348 | class int:
            |       ^^^
            |
        info: Source
         --> main2.py:4:31
          |
        4 | a[: B[A[D[int, list[str | A[B[int]]]]]]] = C().foo()
          |                               ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + from bar import D
        2 |
          - from foo import C
        3 + from foo import C, B, A
        4 |
          - a = C().foo()
        5 + a: B[A[D[int, list[str | A[B[int]]]]]] = C().foo()
        ");
    }

    #[test]
    fn test_auto_import_symbol_imported_from_different_path() {
        let mut test = inlay_hint_test(
            "
            from foo import D

            class Baz: ...

            a = D(Baz)
            ",
        );

        test.with_extra_file(
            "foo/__init__.py",
            "
            from foo.bar import D
                    ",
        );

        test.with_extra_file(
            "foo/bar.py",
            "
            class D[T]:
                def __init__(self, x: type[T]):
                    pass
            ",
        );

        assert_snapshot!(test.inlay_hints(), @"

        from foo import D

        class Baz: ...

        a[: D[Baz]] = D([x=]Baz)

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> foo/bar.py:2:19
          |
        2 |             class D[T]:
          |                   ^
          |
        info: Source
         --> main2.py:6:5
          |
        6 | a[: D[Baz]] = D([x=]Baz)
          |     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:7
          |
        4 | class Baz: ...
          |       ^^^
          |
        info: Source
         --> main2.py:6:7
          |
        6 | a[: D[Baz]] = D([x=]Baz)
          |       ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo/bar.py:3:36
          |
        3 |                 def __init__(self, x: type[T]):
          |                                    ^
          |
        info: Source
         --> main2.py:6:18
          |
        6 | a[: D[Baz]] = D([x=]Baz)
          |                  ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        3 |
        4 | class Baz: ...
        5 |
          - a = D(Baz)
        6 + a: D[Baz] = D(x=Baz)
        ");
    }

    #[test]
    fn test_auto_import_typing_literal() {
        let mut test = inlay_hint_test(
            r#"
            from typing import Any

            def foo(x: Any):
                a = getattr(x, 'foo', "some")
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        from typing import Any

        def foo(x: Any):
            a[: Any | Literal["some"]] = getattr(x, 'foo', "some")

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:166:7
            |
        166 | class Any:
            |       ^^^
            |
        info: Source
         --> main2.py:5:9
          |
        5 |     a[: Any | Literal["some"]] = getattr(x, 'foo', "some")
          |         ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:5:15
          |
        5 |     a[: Any | Literal["some"]] = getattr(x, 'foo', "some")
          |               ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        915 | class str(Sequence[str]):
            |       ^^^
            |
        info: Source
         --> main2.py:5:23
          |
        5 |     a[: Any | Literal["some"]] = getattr(x, 'foo', "some")
          |                       ^^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 |
          - from typing import Any
        2 + from typing import Any, Literal
        3 |
        4 | def foo(x: Any):
          -     a = getattr(x, 'foo', "some")
        5 +     a: Any | Literal["some"] = getattr(x, 'foo', "some")
        "#);
    }

    #[test]
    fn test_auto_import_other_symbols() {
        let mut test = inlay_hint_test(
            r#"
            from foo import foo

            a = foo()
            "#,
        );

        test.with_extra_file(
            "foo.py",
            r#"
        from typing import TypeVar, Any

        def foo() -> dict[TypeVar, Any] | None: ...
        "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from foo import foo

        a[: dict[TypeVar, Any] | None] = foo()

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2947:7
             |
        2947 | class dict(MutableMapping[_KT, _VT]):
             |       ^^^^
             |
        info: Source
         --> main2.py:4:5
          |
        4 | a[: dict[TypeVar, Any] | None] = foo()
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:211:7
            |
        211 | class TypeVar:
            |       ^^^^^^^
            |
        info: Source
         --> main2.py:4:10
          |
        4 | a[: dict[TypeVar, Any] | None] = foo()
          |          ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:166:7
            |
        166 | class Any:
            |       ^^^
            |
        info: Source
         --> main2.py:4:19
          |
        4 | a[: dict[TypeVar, Any] | None] = foo()
          |                   ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:969:11
            |
        969 |     class NoneType:
            |           ^^^^^^^^
            |
        info: Source
         --> main2.py:4:26
          |
        4 | a[: dict[TypeVar, Any] | None] = foo()
          |                          ^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + from typing import TypeVar
        2 + from typing import Any
        3 |
        4 | from foo import foo
        5 |
          - a = foo()
        6 + a: dict[TypeVar, Any] | None = foo()
        ");
    }

    /// Tests that if we have an inlay hint containing two symbols with the same name
    /// from unimported modules, then we add two `import <module>` statements, and
    /// qualify both symbols (<module1.<symbol1>, <module2.<symbol1>).
    #[test]
    fn test_auto_import_same_name_different_modules_both_qualified() {
        let mut test = inlay_hint_test(
            r#"
            from foo import foo

            a = foo()
            "#,
        );

        test.with_extra_file(
            "foo.py",
            r#"
        import bar
        import baz

        def foo() -> bar.A | baz.A:
            return bar.A()
        "#,
        );

        test.with_extra_file(
            "bar.py",
            r#"
            class A: ...
        "#,
        );

        test.with_extra_file(
            "baz.py",
            r#"
            class A: ...
        "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from foo import foo

        a[: bar.A | baz.A] = foo()

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> bar.py:2:19
          |
        2 |             class A: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:5
          |
        4 | a[: bar.A | baz.A] = foo()
          |     ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> baz.py:2:19
          |
        2 |             class A: ...
          |                   ^
          |
        info: Source
         --> main2.py:4:13
          |
        4 | a[: bar.A | baz.A] = foo()
          |             ^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + import bar
        2 + import baz
        3 |
        4 | from foo import foo
        5 |
          - a = foo()
        6 + a: bar.A | baz.A = foo()
        ");
    }

    /// Tests that if we have an inlay hint containing two symbols with the same name
    /// from two modules, one which is imported already via a "import from" statement,
    /// then we still add two `import <module>` statements.
    ///
    /// We also show here that we don't add repeated import statements.
    #[test]
    fn test_auto_import_same_name_different_modules_one_qualified() {
        let mut test = inlay_hint_test(
            r#"
               from foo import foo
               from bar import B

               a = foo()
               "#,
        );

        test.with_extra_file(
            "foo.py",
            r#"
           import bar
           import baz

           def foo() -> bar.A | baz.A | list[bar.A | baz.A]:
               return bar.A()
           "#,
        );

        test.with_extra_file(
            "bar.py",
            r#"
               class A: ...
               class B: ...
           "#,
        );

        test.with_extra_file(
            "baz.py",
            r#"
               class A: ...
           "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from foo import foo
        from bar import B

        a[: bar.A | baz.A | list[bar.A | baz.A]] = foo()

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> bar.py:2:22
          |
        2 |                class A: ...
          |                      ^
          |
        info: Source
         --> main2.py:5:5
          |
        5 | a[: bar.A | baz.A | list[bar.A | baz.A]] = foo()
          |     ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> baz.py:2:22
          |
        2 |                class A: ...
          |                      ^
          |
        info: Source
         --> main2.py:5:13
          |
        5 | a[: bar.A | baz.A | list[bar.A | baz.A]] = foo()
          |             ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:5:21
          |
        5 | a[: bar.A | baz.A | list[bar.A | baz.A]] = foo()
          |                     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> bar.py:2:22
          |
        2 |                class A: ...
          |                      ^
          |
        info: Source
         --> main2.py:5:26
          |
        5 | a[: bar.A | baz.A | list[bar.A | baz.A]] = foo()
          |                          ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> baz.py:2:22
          |
        2 |                class A: ...
          |                      ^
          |
        info: Source
         --> main2.py:5:34
          |
        5 | a[: bar.A | baz.A | list[bar.A | baz.A]] = foo()
          |                                  ^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        1 + import bar
        2 + import baz
        3 |
        4 | from foo import foo
        5 | from bar import B
        6 |
          - a = foo()
        7 + a: bar.A | baz.A | list[bar.A | baz.A] = foo()
        ");
    }

    /// Tests that if we have an inlay hint containing a symbol that is referenced
    /// in another module, that we qualify the inlay hint symbol with the module name,
    /// so we don't accidentally reference the in scope symbol.
    #[test]
    fn test_auto_import_symbol_in_scope_same_name() {
        let mut test = inlay_hint_test(
            r#"
                from dataclasses import dataclass
                import foo

                class A: ...

                @dataclass
                class B[T]:
                    x: T

                b = B(foo.A())
               "#,
        );

        test.with_extra_file(
            "foo.py",
            r#"
            class A: ...
           "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from dataclasses import dataclass
        import foo

        class A: ...

        @dataclass
        class B[T]:
            x: T

        b[: B[A]] = B([x=]foo.A())

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:8:7
          |
        8 | class B[T]:
          |       ^
          |
        info: Source
          --> main2.py:11:5
           |
        11 | b[: B[A]] = B([x=]foo.A())
           |     ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:2:19
          |
        2 |             class A: ...
          |                   ^
          |
        info: Source
          --> main2.py:11:7
           |
        11 | b[: B[A]] = B([x=]foo.A())
           |       ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:9:5
          |
        9 |     x: T
          |     ^
          |
        info: Source
          --> main2.py:11:16
           |
        11 | b[: B[A]] = B([x=]foo.A())
           |                ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        8  | class B[T]:
        9  |     x: T
        10 |
           - b = B(foo.A())
        11 + b: B[foo.A] = B(x=foo.A())
        ");
    }

    #[test]
    fn test_auto_import_enum_member() {
        let mut test = inlay_hint_test(
            r#"
            from test import Color

            x = Color.RED
            "#,
        );

        test.with_extra_file(
            "test.py",
            r#"
            from enum import Enum

            class Color(Enum):
                RED = 1
                BLUE = 2
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from test import Color

        x[: Literal[Color.RED]] = Color.RED

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:4:5
          |
        4 | x[: Literal[Color.RED]] = Color.RED
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> test.py:4:19
          |
        4 |             class Color(Enum):
          |                   ^^^^^
          |
        info: Source
         --> main2.py:4:13
          |
        4 | x[: Literal[Color.RED]] = Color.RED
          |             ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> test.py:5:17
          |
        5 |                 RED = 1
          |                 ^^^
          |
        info: Source
         --> main2.py:4:19
          |
        4 | x[: Literal[Color.RED]] = Color.RED
          |                   ^^^
          |
        ");
    }

    /// Regression test for astral-sh/ty#3313: applying the inlay hint on `y`
    /// previously added `Inner` to `from module import Outer`, but `Inner` is
    /// a nested class inside `Outer`, not a top-level symbol of `module`.
    #[test]
    fn test_auto_import_nested_class() {
        let mut test = inlay_hint_test(
            r#"
            from module import Outer


            def wrap[T](x: T) -> list[T]:
                return [x]

            y = wrap(Outer.Inner())
            "#,
        );

        test.with_extra_file(
            "module.py",
            r#"
            class Outer:
                class Inner: ...
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from module import Outer


        def wrap[T](x: T) -> list[T]:
            return [x]

        y[: list[Inner]] = wrap([x=]Outer.Inner())

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2829:7
             |
        2829 | class list(MutableSequence[_T]):
             |       ^^^^
             |
        info: Source
         --> main2.py:8:5
          |
        8 | y[: list[Inner]] = wrap([x=]Outer.Inner())
          |     ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> module.py:3:23
          |
        3 |                 class Inner: ...
          |                       ^^^^^
          |
        info: Source
         --> main2.py:8:10
          |
        8 | y[: list[Inner]] = wrap([x=]Outer.Inner())
          |          ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:13
          |
        5 | def wrap[T](x: T) -> list[T]:
          |             ^
          |
        info: Source
         --> main2.py:8:26
          |
        8 | y[: list[Inner]] = wrap([x=]Outer.Inner())
          |                          ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: Inlay hint edits
        --> main.py:1:1
        5 | def wrap[T](x: T) -> list[T]:
        6 |     return [x]
        7 |
          - y = wrap(Outer.Inner())
        8 + y = wrap(x=Outer.Inner())
        ");
    }

    #[test]
    fn test_auto_import_enum_member_unimported_class() {
        let mut test = inlay_hint_test(
            r#"
            import test

            x = test.Color.RED
            "#,
        );

        test.with_extra_file(
            "test.py",
            r#"
            from enum import Enum

            class Color(Enum):
                RED = 1
                BLUE = 2
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        import test

        x[: Literal[Color.RED]] = test.Color.RED

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:487:1
            |
        487 | Literal: _SpecialForm
            | ^^^^^^^
            |
        info: Source
         --> main2.py:4:5
          |
        4 | x[: Literal[Color.RED]] = test.Color.RED
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> test.py:4:19
          |
        4 |             class Color(Enum):
          |                   ^^^^^
          |
        info: Source
         --> main2.py:4:13
          |
        4 | x[: Literal[Color.RED]] = test.Color.RED
          |             ^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> test.py:5:17
          |
        5 |                 RED = 1
          |                 ^^^
          |
        info: Source
         --> main2.py:4:19
          |
        4 | x[: Literal[Color.RED]] = test.Color.RED
          |                   ^^^
          |
        ");
    }

    #[test]
    fn test_auto_import_method_returning_nested_class() {
        let mut test = inlay_hint_test(
            r#"
            from module import Outer

            x = Outer().make()
            "#,
        );

        test.with_extra_file(
            "module.py",
            r#"
            class Outer:
                class Inner: ...

                def make(self) -> "Outer.Inner":
                    return Outer.Inner()
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"

        from module import Outer

        x[: Inner] = Outer().make()

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> module.py:3:23
          |
        3 |                 class Inner: ...
          |                       ^^^^^
          |
        info: Source
         --> main2.py:4:5
          |
        4 | x[: Inner] = Outer().make()
          |     ^^^^^
          |
        ");
    }

    #[test]
    fn test_auto_import_same_file_method_returning_nested_class() {
        let mut test = inlay_hint_test(
            r#"
            class Outer:
                class Inner: ...

                def make(self) -> "Outer.Inner":
                    return Outer.Inner()

            x = Outer().make()
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"

        class Outer:
            class Inner: ...

            def make(self) -> "Outer.Inner":
                return Outer.Inner()

        x[: Inner] = Outer().make()

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:11
          |
        3 |     class Inner: ...
          |           ^^^^^
          |
        info: Source
         --> main2.py:8:5
          |
        8 | x[: Inner] = Outer().make()
          |     ^^^^^
          |
        "#);
    }

    struct InlayHintLocationDiagnostic {
        source: FileRange,
        target: FileRange,
    }

    impl InlayHintLocationDiagnostic {
        fn new(source: FileRange, target: &NavigationTarget) -> Self {
            Self {
                source,
                target: FileRange::new(target.file(), target.focus_range()),
            }
        }
    }

    impl IntoDiagnostic for InlayHintLocationDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut source = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Source");

            source.annotate(Annotation::primary(
                Span::from(self.source.file()).with_range(self.source.range()),
            ));

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("inlay-hint-location")),
                Severity::Info,
                "Inlay Hint Target".to_string(),
            );

            main.annotate(Annotation::primary(
                Span::from(self.target.file()).with_range(self.target.range()),
            ));

            main.sub(source);

            main
        }
    }

    struct InlayHintEditDiagnostic<'a> {
        file: File,
        first_edit: &'a InlayHintTextEdit,
        rest: &'a [InlayHintTextEdit],
    }

    impl<'a> InlayHintEditDiagnostic<'a> {
        fn new(
            file: File,
            first_edit: &'a InlayHintTextEdit,
            rest: &'a [InlayHintTextEdit],
        ) -> Self {
            Self {
                file,
                first_edit,
                rest,
            }
        }
    }

    impl IntoDiagnostic for InlayHintEditDiagnostic<'_> {
        fn into_diagnostic(self) -> Diagnostic {
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("inlay-hint-edit")),
                Severity::Info,
                "Inlay hint edits".to_string(),
            );

            let mut annotation = Annotation::primary(Span::from(self.file));
            annotation.hide_snippet(true);
            main.annotate(annotation);

            // These fixes aren't actually safe but using `safe` has the benefit over unsafe
            // that it doesn't render a noisy "This is an unsafe fix" note
            let fix = Fix::safe_edits(
                self.first_edit.to_fix_edit(),
                self.rest.iter().map(InlayHintTextEdit::to_fix_edit),
            );

            main.set_fix(fix);

            main
        }
    }

    impl InlayHintTextEdit {
        fn to_fix_edit(&self) -> Edit {
            if self.range.is_empty() {
                Edit::insertion(self.new_text.clone(), self.range.start())
            } else {
                Edit::range_replacement(self.new_text.clone(), self.range)
            }
        }
    }
}
