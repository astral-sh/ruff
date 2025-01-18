use ruff_annotate_snippets::{Level, Renderer, Snippet};

fn main() {
    let message =
        Level::Error
            .title("mismatched types")
            .id("E0308")
            .snippet(
                Snippet::source("        slices: vec![\"A\",")
                    .line_start(13)
                    .origin("src/multislice.rs")
                    .annotation(Level::Error.span(21..24).label(
                        "expected struct `annotate_snippets::snippet::Slice`, found reference",
                    )),
            )
            .footer(Level::Note.title(
                "expected type: `snippet::Annotation`\n   found type: `__&__snippet::Annotation`",
            ));

    let renderer = Renderer::styled();
    anstream::println!("{}", renderer.render(message));
}
