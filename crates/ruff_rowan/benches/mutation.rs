use ruff_rowan::{
    raw_language::{LiteralExpression, RawLanguageKind, RawLanguageRoot, RawSyntaxTreeBuilder},
    AstNode, AstNodeExt, BatchMutationExt, SyntaxNodeCast,
};

/// ```
/// 0: ROOT@0..1
///     0: LITERAL_EXPRESSION@0..1
///         0: STRING_TOKEN@0..1 "a" [] []
/// ```
fn tree_one(a: &str) -> (RawLanguageRoot, String) {
    let mut builder = RawSyntaxTreeBuilder::new();
    builder
        .start_node(RawLanguageKind::ROOT)
        .start_node(RawLanguageKind::LITERAL_EXPRESSION)
        .token(RawLanguageKind::STRING_TOKEN, a)
        .finish_node()
        .finish_node();
    let root = builder.finish().cast::<RawLanguageRoot>().unwrap();
    let s = format!("{:#?}", root.syntax());
    (root, s)
}

fn find(root: &RawLanguageRoot, name: &str) -> LiteralExpression {
    root.syntax()
        .descendants()
        .find(|x| x.kind() == RawLanguageKind::LITERAL_EXPRESSION && x.text_trimmed() == name)
        .unwrap()
        .cast::<LiteralExpression>()
        .unwrap()
}

fn clone_detach(root: &RawLanguageRoot, name: &str) -> LiteralExpression {
    root.syntax()
        .descendants()
        .find(|x| x.kind() == RawLanguageKind::LITERAL_EXPRESSION && x.text_trimmed() == name)
        .unwrap()
        .detach()
        .cast::<LiteralExpression>()
        .unwrap()
}

fn mutation_replace_node() -> usize {
    let (before, _) = tree_one("a");
    let (expected, _) = tree_one("b");

    let a = find(&before, "a");
    let b = clone_detach(&expected, "b");

    let root = before.replace_node(a, b).unwrap();

    root.syntax().descendants().count()
}

fn mutation_batch() -> usize {
    let (before, _) = tree_one("a");
    let (expected, _) = tree_one("b");

    let a = find(&before, "a");
    let b = clone_detach(&expected, "b");

    let mut batch = before.begin();
    batch.replace_node(a, b);
    let root = batch.commit();

    root.descendants().count()
}

iai::main!(mutation_replace_node, mutation_batch);
