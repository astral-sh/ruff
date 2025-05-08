use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_ast::{AnyNodeRef, Identifier};
use ruff_text_size::TextSize;

use crate::Db;

#[derive(Debug, Clone)]
pub struct Completion {
    pub label: String,
}

pub fn completion(db: &dyn Db, file: File, _offset: TextSize) -> Vec<Completion> {
    let parsed = parsed_module(db.upcast(), file);
    identifiers(parsed.syntax().into())
        .into_iter()
        .map(|label| Completion { label })
        .collect()
}

fn identifiers(node: AnyNodeRef) -> Vec<String> {
    struct Visitor {
        identifiers: Vec<String>,
    }

    impl<'a> SourceOrderVisitor<'a> for Visitor {
        fn visit_identifier(&mut self, id: &'a Identifier) {
            self.identifiers.push(id.id.as_str().to_string());
        }
    }

    let mut visitor = Visitor {
        identifiers: vec![],
    };
    node.visit_source_order(&mut visitor);
    visitor.identifiers.sort();
    visitor.identifiers.dedup();
    visitor.identifiers
}
