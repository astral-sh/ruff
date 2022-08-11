use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_parser::parser;

use crate::check_ast::check_ast;
use crate::check_lines::check_lines;
use crate::message::Message;
use crate::{cache, fs};

pub fn check_path(path: &Path, mode: &cache::Mode) -> Result<Vec<Message>> {
    // Check the cache.
    if let Some(messages) = cache::get(path, mode) {
        debug!("Cache hit for: {}", path.to_string_lossy());
        return Ok(messages);
    }

    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Run the parser.
    let python_ast = parser::parse_program(&contents)?;

    // Run the linter.
    let messages: Vec<Message> = check_ast(&python_ast)
        .into_iter()
        .chain(check_lines(&contents))
        .map(|check| Message {
            kind: check.kind,
            location: check.location,
            filename: path.to_string_lossy().to_string(),
        })
        .filter(|message| !message.is_inline_ignored())
        .collect();
    cache::set(path, &messages, mode);

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustpython_parser::ast::Location;

    use crate::cache;
    use crate::checks::CheckKind::{DuplicateArgumentName, IfTuple, ImportStarUsage, LineTooLong};
    use crate::linter::check_path;
    use crate::message::Message;

    #[test]
    fn duplicate_argument_name() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/duplicate_argument_name.py"),
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: DuplicateArgumentName,
                location: Location::new(1, 25),
                filename: "./resources/test/src/duplicate_argument_name.py".to_string(),
            },
            Message {
                kind: DuplicateArgumentName,
                location: Location::new(5, 28),
                filename: "./resources/test/src/duplicate_argument_name.py".to_string(),
            },
            Message {
                kind: DuplicateArgumentName,
                location: Location::new(9, 27),
                filename: "./resources/test/src/duplicate_argument_name.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 1..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn if_tuple() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/if_tuple.py"),
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: IfTuple,
                location: Location::new(1, 1),
                filename: "./resources/test/src/if_tuple.py".to_string(),
            },
            Message {
                kind: IfTuple,
                location: Location::new(7, 5),
                filename: "./resources/test/src/if_tuple.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 1..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn import_star_usage() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/import_star_usage.py"),
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: ImportStarUsage,
                location: Location::new(1, 1),
                filename: "./resources/test/src/import_star_usage.py".to_string(),
            },
            Message {
                kind: ImportStarUsage,
                location: Location::new(2, 1),
                filename: "./resources/test/src/import_star_usage.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 1..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn line_too_long() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/line_too_long.py"),
            &cache::Mode::None,
        )?;
        let expected = vec![Message {
            kind: LineTooLong,
            location: Location::new(3, 88),
            filename: "./resources/test/src/line_too_long.py".to_string(),
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 1..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }
}
