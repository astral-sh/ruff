#[cfg(test)]
mod tests {

    use crate::{
        lexer::lex,
        parser::{Parser, Program},
        Mode,
    };
    use insta::assert_debug_snapshot;

    fn parse(src: &str) -> Program {
        let mode = Mode::Module;
        let lexer = lex(src, mode);
        let parser = Parser::new(src, mode, lexer.collect());
        let program = parser.parse_program();

        assert_eq!(&program.parse_errors, &[]);
        program
    }

    #[test]
    fn parse_with_stmt() {
        assert_debug_snapshot!(parse(
            "
with x:
    ...
with x, y:
    ...
with open() as f:
    ...
with f() as x.attr:
    pass
with x as X, y as Y, z as Z:
    ...
with (x, z as Y, y,):
    ...
with (a) as f:
    ...
with ((a) as f, 1):
    ...
with a:
    yield a, b
with (yield 1):
    ...
with (yield from 1):
    ...
with (a := 1):
    ...
with (open('bla.txt')), (open('bla.txt')):
    pass
with (a := 1, x):
    ...
with (p / 'new_file').open('wb'): ...
"
        ));
    }
}
