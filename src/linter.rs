use std::path::Path;

use anyhow::Result;
use log::debug;

use crate::checker::check_ast;
use crate::message::Message;
use crate::{cache, parser};

pub fn check_path(path: &Path, mode: &cache::Mode) -> Result<Vec<Message>> {
    // Check the cache.
    if let Some(messages) = cache::get(path, mode) {
        debug!("Cache hit for: {}", path.to_string_lossy());
        return Ok(messages);
    }

    // Run the linter.
    let python_ast = parser::parse(path)?;
    let messages: Vec<Message> = check_ast(&python_ast)
        .into_iter()
        .map(|check| Message {
            kind: check.kind,
            location: check.location,
            filename: path.to_string_lossy().to_string(),
        })
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
    use crate::checks::CheckKind::{DuplicateArgumentName, IfTuple, ImportStarUsage};
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
                location: Location::new(5, 9),
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
        let expected = vec![Message {
            kind: ImportStarUsage,
            location: Location::new(1, 1),
            filename: "./resources/test/src/import_star_usage.py".to_string(),
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 1..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }
}
