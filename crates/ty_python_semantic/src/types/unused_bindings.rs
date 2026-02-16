use crate::semantic_index::scope::ScopeKind;
use crate::{Db, SemanticModel};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{
    SourceOrderVisitor, walk_expr, walk_parameter, walk_parameter_with_default, walk_pattern,
    walk_stmt,
};
use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextRange};

fn is_dunder_name(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

fn should_mark_unnecessary(scope_kind: ScopeKind, name: &str) -> bool {
    if name.starts_with('_') || matches!(name, "self" | "cls") || is_dunder_name(name) {
        return false;
    }

    // Keep this local-scope only to avoid false positives for bindings that can
    // be observed or referenced indirectly from module/class contexts.
    match scope_kind {
        ScopeKind::Function | ScopeKind::Lambda | ScopeKind::Comprehension => true,
        ScopeKind::Module | ScopeKind::Class | ScopeKind::TypeParams | ScopeKind::TypeAlias => {
            false
        }
    }
}

/// Check whether a symbol is unused within its containing scope and should be marked as unnecessary.
/// Returns `Some(true)` if unused and should be marked, `Some(false)` otherwise, or `None` if the symbol cannot be found.
fn is_symbol_unnecessary_in_scope(
    model: &SemanticModel<'_>,
    scope_node: ast::AnyNodeRef<'_>,
    name: &str,
) -> Option<bool> {
    let file = model.file();
    let file_scope = model.scope(scope_node)?;
    let index = crate::semantic_index::semantic_index(model.db(), file);
    let scope = index.scope(file_scope);

    if !should_mark_unnecessary(scope.kind(), name) {
        return Some(false);
    }

    let place_table = index.place_table(file_scope);
    let symbol_id = place_table.symbol_id(name)?;
    let symbol = place_table.symbol(symbol_id);

    // Global and nonlocal assignments target bindings from outer scopes.
    // Treat them as externally managed to avoid false positives here.
    if symbol.is_global() || symbol.is_nonlocal() {
        return Some(false);
    }

    Some(!symbol.is_used())
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnusedBinding {
    pub range: TextRange,
    pub name: String,
}

#[salsa::tracked(returns(ref))]
pub fn unused_bindings(db: &dyn Db, file: ruff_db::files::File) -> Vec<UnusedBinding> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);

    let mut collector = UnusedBindingCollector::new(&model);
    collector.visit_body(parsed.suite());
    collector
        .unused_bindings
        .sort_unstable_by_key(|binding| binding.range.start());
    collector
        .unused_bindings
        .dedup_by_key(|binding| binding.range);

    collector.unused_bindings
}

struct UnusedBindingCollector<'db> {
    model: &'db SemanticModel<'db>,
    unused_bindings: Vec<UnusedBinding>,
    in_target_creating_definition: bool,
}

impl<'db> UnusedBindingCollector<'db> {
    fn new(model: &'db SemanticModel<'db>) -> Self {
        Self {
            model,
            unused_bindings: Vec::new(),
            in_target_creating_definition: false,
        }
    }

    fn add_unused_binding(&mut self, range: TextRange, name: &str) {
        self.unused_bindings.push(UnusedBinding {
            range,
            name: name.to_string(),
        });
    }

    fn mark_pattern_binding_if_unused(&mut self, name: &ast::Identifier) {
        if let Some(true) = is_symbol_unnecessary_in_scope(
            self.model,
            ast::AnyNodeRef::from(name),
            name.id.as_str(),
        ) {
            self.add_unused_binding(name.range(), name.id.as_str());
        }
    }

    fn with_target_creating_definition(&mut self, f: impl FnOnce(&mut Self)) {
        let prev = self.in_target_creating_definition;
        self.in_target_creating_definition = true;
        f(self);
        self.in_target_creating_definition = prev;
    }
}

impl SourceOrderVisitor<'_> for UnusedBindingCollector<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::Assign(assignment) => {
                self.with_target_creating_definition(|this| {
                    for target in &assignment.targets {
                        this.visit_expr(target);
                    }
                });

                self.visit_expr(&assignment.value);
            }
            ast::Stmt::AnnAssign(assignment) => {
                self.with_target_creating_definition(|this| {
                    this.visit_expr(&assignment.target);
                });

                self.visit_expr(&assignment.annotation);
                if let Some(value) = &assignment.value {
                    self.visit_expr(value);
                }
            }
            ast::Stmt::For(for_stmt) => {
                self.with_target_creating_definition(|this| {
                    this.visit_expr(&for_stmt.target);
                });

                self.visit_expr(&for_stmt.iter);
                self.visit_body(&for_stmt.body);
                self.visit_body(&for_stmt.orelse);
            }
            ast::Stmt::With(with_stmt) => {
                for item in &with_stmt.items {
                    self.visit_expr(&item.context_expr);
                    if let Some(expr) = &item.optional_vars {
                        self.with_target_creating_definition(|this| {
                            this.visit_expr(expr);
                        });
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
                            if let Some(name) = &except_handler.name
                                && let Some(true) = is_symbol_unnecessary_in_scope(
                                    self.model,
                                    ast::AnyNodeRef::from(except_handler),
                                    name.id.as_str(),
                                )
                            {
                                self.add_unused_binding(name.range(), name.id.as_str());
                            }
                            self.visit_body(&except_handler.body);
                        }
                    }
                }
                self.visit_body(&try_stmt.orelse);
                self.visit_body(&try_stmt.finalbody);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match expr {
            ast::Expr::Name(name) => {
                if self.in_target_creating_definition
                    && name.ctx.is_store()
                    && let Some(true) = is_symbol_unnecessary_in_scope(
                        self.model,
                        ast::AnyNodeRef::from(name),
                        name.id.as_str(),
                    )
                {
                    self.add_unused_binding(name.range(), name.id.as_str());
                }
                walk_expr(self, expr);
            }
            ast::Expr::Named(named) => {
                self.with_target_creating_definition(|this| {
                    this.visit_expr(&named.target);
                });

                self.visit_expr(&named.value);
            }
            _ => walk_expr(self, expr),
        }
    }

    fn visit_pattern(&mut self, pattern: &ast::Pattern) {
        match pattern {
            ast::Pattern::MatchAs(pattern_as) => {
                if let Some(nested_pattern) = &pattern_as.pattern {
                    self.visit_pattern(nested_pattern);
                }
                if let Some(name) = &pattern_as.name {
                    self.mark_pattern_binding_if_unused(name);
                }
            }
            ast::Pattern::MatchMapping(pattern_mapping) => {
                for (key, nested_pattern) in
                    pattern_mapping.keys.iter().zip(&pattern_mapping.patterns)
                {
                    self.visit_expr(key);
                    self.visit_pattern(nested_pattern);
                }
                if let Some(rest_name) = &pattern_mapping.rest {
                    self.mark_pattern_binding_if_unused(rest_name);
                }
            }
            ast::Pattern::MatchStar(pattern_star) => {
                if let Some(rest_name) = &pattern_star.name {
                    self.mark_pattern_binding_if_unused(rest_name);
                }
            }
            _ => walk_pattern(self, pattern),
        }
    }

    fn visit_comprehension(&mut self, comprehension: &ast::Comprehension) {
        self.with_target_creating_definition(|this| {
            this.visit_expr(&comprehension.target);
        });

        self.visit_expr(&comprehension.iter);
        for if_clause in &comprehension.ifs {
            self.visit_expr(if_clause);
        }
    }

    fn visit_parameter(&mut self, parameter: &ast::Parameter) {
        if let Some(true) = is_symbol_unnecessary_in_scope(
            self.model,
            ast::AnyNodeRef::from(parameter),
            parameter.name.id.as_str(),
        ) {
            self.add_unused_binding(parameter.name.range(), parameter.name.id.as_str());
        }
        walk_parameter(self, parameter);
    }

    fn visit_parameter_with_default(&mut self, parameter_with_default: &ast::ParameterWithDefault) {
        if let Some(true) = is_symbol_unnecessary_in_scope(
            self.model,
            ast::AnyNodeRef::from(parameter_with_default),
            parameter_with_default.name().id.as_str(),
        ) {
            self.add_unused_binding(
                parameter_with_default.name().range(),
                parameter_with_default.name().id.as_str(),
            );
        }
        walk_parameter_with_default(self, parameter_with_default);
    }
}

#[cfg(test)]
mod tests {
    use super::unused_bindings;
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;

    fn collect_unused_names(source: &str) -> anyhow::Result<Vec<String>> {
        let db = TestDbBuilder::new()
            .with_file("/src/main.py", source)
            .build()?;
        let file = system_path_to_file(&db, "/src/main.py").unwrap();
        let mut names = unused_bindings(&db, file)
            .iter()
            .map(|binding| binding.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    #[test]
    fn captures_safe_local_binding_kinds() -> anyhow::Result<()> {
        let source = r#"def f():
    used_assign, dead_assign = (1, 2)
    print(used_assign)

    for used_loop, dead_loop in [(1, 2)]:
        print(used_loop)

    with open("x") as dead_with:
        pass

    try:
        1 / 0
    except Exception as dead_exc:
        pass

    if (dead_walrus := 1):
        pass

    [1 for dead_comp in range(3)]
    [ok_comp for ok_comp, dead_comp2 in [(1, 2)]]

    match {"x": 1, "y": 2}:
        case {"x": used_match, **dead_rest}:
            print(used_match)
        case [used_star, *dead_star] as dead_as:
            print(used_star)
"#;

        let names = collect_unused_names(source)?;
        assert_eq!(
            names,
            vec![
                "dead_as",
                "dead_assign",
                "dead_comp",
                "dead_comp2",
                "dead_exc",
                "dead_loop",
                "dead_rest",
                "dead_star",
                "dead_walrus",
                "dead_with",
            ]
        );
        Ok(())
    }

    #[test]
    fn skips_module_class_placeholder_and_dunder_bindings() -> anyhow::Result<()> {
        let source = r#"_module_dead = 1

class C:
    __private_dead = 1

    def method(self):
        local_dead = 1
        _ = 2
        __dunder__ = 3
        return 0
"#;

        let names = collect_unused_names(source)?;
        assert_eq!(names, vec!["local_dead"]);
        Ok(())
    }

    #[test]
    fn skips_global_and_nonlocal_assignments() -> anyhow::Result<()> {
        let source = r#"global_value = 0

def mutate_global():
    global global_value
    global_value = 1
    local_dead = 1

def outer():
    captured = 0

    def inner():
        nonlocal captured
        captured = 1

    inner()
    return captured
"#;

        let names = collect_unused_names(source)?;
        assert_eq!(names, vec!["local_dead"]);
        Ok(())
    }

    #[test]
    fn captures_unused_function_and_lambda_parameters() -> anyhow::Result<()> {
        let source = r#"def fn(used, dead, _ignored, __dunder__):
    return used

def fn_defaults(a, b=1, *, c=2, d):
    return a + d

lam = lambda x, y, z=1: x + z
"#;

        let names = collect_unused_names(source)?;
        assert_eq!(names, vec!["b", "c", "dead", "y"]);
        Ok(())
    }
}
