use ruff_python_ast::{self as ast};

use crate::types::{KnownClass, Type, infer::TypeInferenceBuilder};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// This is the only method exposed to and called by `builder.rs`.
    /// All other methods are handled privately.
    /// This may change in the future if a different interface is needed.
    pub(crate) fn post_inference_pytest_check_function(
        &self,
        _ty: Type<'db>,
        node: &ast::StmtFunctionDef,
    ) {
        self.analyze_function_def(node);
    }

    fn analyze_function_def(&self, node: &ast::StmtFunctionDef) {
        for decorator in &node.decorator_list {
            self.analyze_decorator(decorator);
        }
    }

    fn analyze_decorator(&self, decorator: &ast::Decorator) {
        if let ast::Expr::Call(decorator_call) = &decorator.expression
            && self.expression_type(&decorator_call.func)
                == KnownClass::PytestParametrizeMarkDecorator
                    .to_instance(self.db(), self.program_environment())
            && let Some(argnames) = decorator_call.arguments.find_argument("argnames", 0)
        {
            let _ = self.parse_argnames_expression(argnames.value());
        }
    }
}
