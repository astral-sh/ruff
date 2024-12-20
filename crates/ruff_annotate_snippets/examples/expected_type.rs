use ruff_annotate_snippets::{Level, Renderer, Snippet};

fn main() {
    let source = r#"                annotations: vec![SourceAnnotation {
                label: "expected struct `annotate_snippets::snippet::Slice`, found reference"
                    ,
                range: <22, 25>,"#;
    let message = Level::Error.title("expected type, found `22`").snippet(
        Snippet::source(source)
            .line_start(26)
            .origin("examples/footer.rs")
            .fold(true)
            .annotation(
                Level::Error
                    .span(193..195)
                    .label("expected struct `annotate_snippets::snippet::Slice`, found reference"),
            )
            .annotation(Level::Info.span(34..50).label("while parsing this struct")),
    );

    let renderer = Renderer::styled();
    anstream::println!("{}", renderer.render(message));
}
