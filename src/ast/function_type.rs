use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::Expr;

use crate::ast::helpers::{
    collect_call_paths, dealias_call_path, match_call_path, to_module_and_member,
};
use crate::ast::types::{Scope, ScopeKind};

const CLASS_METHODS: [&str; 3] = ["__new__", "__init_subclass__", "__class_getitem__"];
const METACLASS_BASES: [(&str, &str); 2] = [("", "type"), ("abc", "ABCMeta")];

pub enum FunctionType {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
}

/// Classify a function based on its scope, name, and decorators.
pub fn classify(
    scope: &Scope,
    name: &str,
    decorator_list: &[Expr],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
    classmethod_decorators: &[String],
    staticmethod_decorators: &[String],
) -> FunctionType {
    let ScopeKind::Class(scope) = &scope.kind else {
        return FunctionType::Function;
    };
    // Special-case class method, like `__new__`.
    if CLASS_METHODS.contains(&name)
        || scope.bases.iter().any(|expr| {
            // The class itself extends a known metaclass, so all methods are class methods.
            let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
            METACLASS_BASES
                .iter()
                .any(|(module, member)| match_call_path(&call_path, module, member, from_imports))
        })
        || decorator_list.iter().any(|expr| {
            // The method is decorated with a class method decorator (like `@classmethod`).
            let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
            classmethod_decorators.iter().any(|decorator| {
                let (module, member) = to_module_and_member(decorator);
                match_call_path(&call_path, module, member, from_imports)
            })
        })
    {
        FunctionType::ClassMethod
    } else if decorator_list.iter().any(|expr| {
        // The method is decorated with a static method decorator (like
        // `@staticmethod`).
        let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
        staticmethod_decorators.iter().any(|decorator| {
            let (module, member) = to_module_and_member(decorator);
            match_call_path(&call_path, module, member, from_imports)
        })
    }) {
        FunctionType::StaticMethod
    } else {
        // It's an instance method.
        FunctionType::Method
    }
}
