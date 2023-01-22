/// Platform-independent snapshot assertion
#[macro_export]
macro_rules! assert_yaml_snapshot {
    ( $($args: expr),+) => {
        let line_sep = if cfg!(windows) { "\r\n" } else { "\n" };

        // adjust snapshot file for platform
        let mut settings = insta::Settings::clone_current();
        settings.add_redaction("[].fix.content", insta::dynamic_redaction(move |value, _path| {
            insta::internals::Content::Seq(
                value.as_str().unwrap().split(line_sep).map(|line| line.into()).collect()
            )
        }));
        settings.bind(|| {
            insta::assert_yaml_snapshot!($($args),+);
        });
    };
}
