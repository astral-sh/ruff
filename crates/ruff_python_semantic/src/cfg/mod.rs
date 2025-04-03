pub mod graph;
pub mod visualize;

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::fs;
    use std::path::PathBuf;

    use crate::cfg::graph::build_cfg;
    use crate::cfg::visualize::draw_cfg;
    use insta;

    use ruff_python_parser::parse_module;
    use ruff_text_size::Ranged;
    use test_case::test_case;

    #[test_case("no_flow.py")]
    #[test_case("jumps.py")]
    fn control_flow_graph(filename: &str) {
        let path = PathBuf::from("resources/test/fixtures/cfg").join(filename);
        let source = fs::read_to_string(path).expect("failed to read file");
        let stmts = parse_module(&source)
            .unwrap_or_else(|err| panic!("failed to parse source: '{source}': {err}"))
            .into_suite();

        let mut output = String::new();

        for (i, stmt) in stmts.into_iter().enumerate() {
            let func = stmt.as_function_def_stmt().expect(
                "Snapshot test for control flow graph should consist only of function definitions",
            );
            let cfg = build_cfg(&func.body);

            let mermaid_graph = draw_cfg(cfg, &source);
            writeln!(
                output,
                "## Function {}\n\
                ### Source\n\
                ```python\n\
                {}\n\
                ```\n\n\
                ### Control Flow Graph\n\
                ```mermaid\n\
                {}\n\
                ```\n",
                i,
                &source[func.range()],
                mermaid_graph,
            )
            .unwrap();
        }

        insta::with_settings!({
            omit_expression => true,
            input_file => filename,
            description => "This is a Mermaid graph. You can use https://mermaid.live to visualize it as a diagram."
        }, {
            insta::assert_snapshot!(format!("{filename}.md"), output);
        });
    }
}
