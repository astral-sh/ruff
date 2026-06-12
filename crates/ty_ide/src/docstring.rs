//! Docstring parsing utilities for language server features.
//!
//! This module provides functionality for extracting structured information from
//! Python docstrings, including parameter documentation for signature help.
//! Supports Google-style, NumPy-style, and reST/Sphinx-style docstrings.
//! There are no formal specifications for any of these formats, so the parsing
//! logic needs to be tolerant of variations.

mod document;
mod markdown;

use indexmap::IndexMap;
use ruff_python_trivia::{PythonWhitespace, leading_indentation};
use ruff_source_file::UniversalNewlines;

use crate::MarkupKind;

/// A docstring which hasn't yet been interpreted or rendered
///
/// Used to ensure handlers of docstrings select a rendering mode.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Docstring(String);

impl Docstring {
    /// Create a new docstring from the raw string literal contents
    pub fn new(raw: String) -> Self {
        Docstring(raw)
    }

    /// Render the docstring to the given markup format
    pub fn render(&self, kind: MarkupKind) -> String {
        match kind {
            MarkupKind::PlainText => self.render_plaintext(),
            MarkupKind::Markdown => self.render_markdown(),
        }
    }

    /// Render the docstring for plaintext display
    pub fn render_plaintext(&self) -> String {
        documentation_trim(&self.0)
    }

    /// Render the docstring for markdown display
    pub fn render_markdown(&self) -> String {
        let trimmed = documentation_trim(&self.0);
        markdown::render(&trimmed)
    }

    /// Extract parameter documentation from popular docstring formats.
    /// Returns a map of parameter names to their documentation.
    pub fn parameter_documentation(&self) -> IndexMap<String, String> {
        document::parameter_documentation(&self.0)
    }
}

/// Text extracted from within a larger docstring.
///
/// Unlike a complete docstring, a fragment has already had its surrounding indentation removed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocstringFragment(String);

impl DocstringFragment {
    pub fn new(raw: &str) -> Self {
        Self(documentation_fragment_trim(raw))
    }

    pub fn render(&self, kind: MarkupKind) -> String {
        match kind {
            MarkupKind::PlainText => self.0.clone(),
            MarkupKind::Markdown => self.render_markdown(),
        }
    }
}

/// Normalizes an extracted docstring fragment without removing meaningful relative indentation.
fn documentation_fragment_trim(docs: &str) -> String {
    let expanded = docs.trim_end().replace('\t', "        ");
    let mut output = String::with_capacity(expanded.len());
    for line in expanded.universal_newlines() {
        output.push_str(line.as_str().trim_whitespace_end());
        output.push('\n');
    }
    output
}

/// Normalizes tabs and trims a docstring as specified in PEP-0257
///
/// See: <https://peps.python.org/pep-0257/#handling-docstring-indentation>
fn documentation_trim(docs: &str) -> String {
    // First apply tab expansion as we don't want tabs in our output
    // (python says tabs are equal to 8 spaces).
    //
    // We also trim off all trailing whitespace here to eliminate trailing newlines so we
    // don't need to handle trailing blank lines later. We can't trim away leading
    // whitespace yet, because we need to identify the first line and handle it specially.
    let expanded = docs.trim_end().replace('\t', "        ");

    // Compute the minimum indention of all non-empty non-first lines
    // and statistics about leading blank lines to help trim them later.
    let mut min_indent = usize::MAX;
    let mut leading_blank_lines = 0;
    let mut is_first_line = true;
    let mut found_non_blank_line = false;
    for line_obj in expanded.universal_newlines() {
        let line = line_obj.as_str();
        let indent = leading_indentation(line);
        if indent == line {
            // Blank line
            if !found_non_blank_line {
                leading_blank_lines += 1;
            }
        } else {
            // Non-blank line
            found_non_blank_line = true;
            // First line doesn't affect min-indent
            if !is_first_line {
                min_indent = min_indent.min(indent.len());
            }
        }
        is_first_line = false;
    }

    let mut output = String::new();
    let mut lines = expanded.universal_newlines();

    // If the first line is non-blank then we need to include it *fully* trimmed
    // As its indentation is ignored (effectively treated as having min_indent).
    if leading_blank_lines == 0 {
        if let Some(first_line) = lines.next() {
            output.push_str(first_line.as_str().trim_whitespace());
            output.push('\n');
        }
    }

    // For the rest of the lines remove the minimum indent (if possible) and trailing whitespace.
    //
    // We computed min_indent by only counting python whitespace, and all python whitespace
    // is ascii, so we can just remove that many bytes from the front.
    for line_obj in lines.skip(leading_blank_lines) {
        let line = line_obj.as_str();
        let trimmed_line = line[min_indent.min(line.len())..].trim_whitespace_end();
        output.push_str(trimmed_line);
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use insta::Settings;
    use insta::assert_snapshot;

    use super::*;

    fn bind_docstring_snapshot_filters() -> impl Drop {
        let mut settings = Settings::clone_current();
        // Markdown hard breaks are encoded as trailing spaces (`"  \n"`), but many editors
        // trim trailing whitespace in string literals. Replace them with `<HB>` in snapshots
        // so tests are stable and the expected output stays readable.
        settings.add_filter("  \n", "<HB>\n");
        settings.bind_to_scope()
    }

    // A nice doctest that is surrounded by prose
    #[test]
    fn dunder_escape() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Here _this_ and ___that__ should be escaped
        Here *this* and **that** should be untouched
        Here `this` and ``that`` should be untouched

        Here `_this_` and ``__that__`` should be untouched
        Here `_this_` ``__that__`` should be untouched
        `_this_too_should_be_untouched_`

        Here `_this_```__that__`` should be untouched but this_is_escaped
        Here ``_this_```__that__` should be untouched but this_is_escaped

        Here `_this_ and _that_ should be escaped (but isn't)
        Here _this_ and _that_` should be escaped
        `Here _this_ and _that_ should be escaped (but isn't)
        Here _this_ and _that_ should be escaped`

        Here ```_is_``__a__`_balanced_``_mess_```
        Here ```_is_`````__a__``_random_````_mess__````
        ```_is_`````__a__``_random_````_mess__````
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r"
        Here \_this\_ and \_\_\_that\_\_ should be escaped<HB>
        Here *this* and **that** should be untouched<HB>
        Here `this` and ``that`` should be untouched<HB>
        <HB>
        Here `_this_` and ``__that__`` should be untouched<HB>
        Here `_this_` ``__that__`` should be untouched<HB>
        `_this_too_should_be_untouched_`<HB>
        <HB>
        Here `_this_```__that__`` should be untouched but this\_is\_escaped<HB>
        Here ``_this_```__that__` should be untouched but this\_is\_escaped<HB>
        <HB>
        Here `_this_ and _that_ should be escaped (but isn't)<HB>
        Here \_this\_ and \_that\_` should be escaped<HB>
        `Here _this_ and _that_ should be escaped (but isn't)<HB>
        Here \_this\_ and \_that\_ should be escaped`<HB>
        <HB>
        Here ```_is_``__a__`_balanced_``_mess_```<HB>
        Here ```_is_`````__a__``\_random\_````_mess__````<HB>
        ```_is_`````__a__``\_random\_````_mess__````
        ");
    }

    #[test]
    fn html_escape() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Parse a URL into 6 components:
        <scheme>://<netloc>/<path>;<params>?<query>#<fragment>

        Markdown code fences keep literal HTML:

        ```text
        <tag attr="value">content</tag>
        ```

        So does `inline <code>`.
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Parse a URL into 6 components:<HB>
        &lt;scheme&gt;://&lt;netloc&gt;/&lt;path&gt;;&lt;params&gt;?&lt;query&gt;#&lt;fragment&gt;<HB>
        <HB>
        Markdown code fences keep literal HTML:<HB>
        <HB>
        ```text
        <tag attr="value">content</tag>
        ```<HB>
        <HB>
        So does `inline <code>`.
        "#);
    }

    // A literal block where the `::` is flush with the paragraph
    // and should become `:`
    #[test]
    fn literal_colon() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Check out this great example code::

            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")

        You love to see it.
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Check out this great example code:  <HB>
        ```````````python
            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")

        ```````````
        You love to see it.
        "#);
    }

    // A literal block where the `::`  with the paragraph
    // and should be erased
    #[test]
    fn literal_space() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Check out this great example code ::

            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")

        You love to see it.
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Check out this great example code  <HB>
        ```````````python
            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")

        ```````````
        You love to see it.
        "#);
    }

    // A literal block where the `::` is floating
    // and the whole line should be deleted
    #[test]
    fn literal_own_line() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Check out this great example code
            ::

            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")

        You love to see it.
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Check out this great example code<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;  <HB>
        ```````````python
            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")

        ```````````
        You love to see it.
        "#);
    }

    // A literal block where the blank lines are missing
    // and I have no idea what Should happen but let's record what Does
    #[test]
    fn literal_squeezed() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Check out this great example code::
            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")
        You love to see it.
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Check out this great example code:<HB>
        ```````````python
            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")
        ```````````
        You love to see it.
        "#);
    }

    // A literal block where the docstring just ends
    // and we should tidy up
    #[test]
    fn literal_flush() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Check out this great example code::

            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")"#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Check out this great example code:  <HB>
        ```````````python
            x_y = "hello"

            if len(x_y) > 4:
                print(x_y)
            else:
                print("too short :(")

            print("done")
        ```````````
        "#);
    }

    // `warning` and several other directives are special languages that should actually
    // still be shown as text and not ```code```.
    #[test]
    fn warning_block() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        The thing you need to understand is that computers are hard.

        .. warning::
            Now listen here buckaroo you might have seen me say computers are hard,
            and though "yeah I know computers are hard but NO you DON'T KNOW.

            Listen:

            - Computers
            - Are
            - Hard

            Ok!?!?!?
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        The thing you need to understand is that computers are hard.<HB>
        <HB>
        **Warning:**<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;Now listen here buckaroo you might have seen me say computers are hard,<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;and though "yeah I know computers are hard but NO you DON'T KNOW.<HB>
        <HB>
        &nbsp;&nbsp;&nbsp;&nbsp;Listen:<HB>
        <HB>
        &nbsp;&nbsp;&nbsp;&nbsp;- Computers<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;- Are<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;- Hard<HB>
        <HB>
        &nbsp;&nbsp;&nbsp;&nbsp;Ok!?!?!?
        "#);
    }

    // `warning` and several other directives are special languages that should actually
    // still be shown as text and not ```code```.
    #[test]
    fn version_blocks() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Some much-updated docs

        .. version-added:: 3.0
           Function added

        .. version-changed:: 4.0
           The `spam` argument was added
        .. version-changed:: 4.1
           The `spam` argument is considered evil now.

           You really shouldnt use it

        And that's the docs
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        Some much-updated docs<HB>
        <HB>
        **Added in version 3.0:**<HB>
        &nbsp;&nbsp;&nbsp;Function added<HB>
        <HB>
        **Changed in version 4.0:**<HB>
        &nbsp;&nbsp;&nbsp;The `spam` argument was added<HB>
        **Changed in version 4.1:**<HB>
        &nbsp;&nbsp;&nbsp;The `spam` argument is considered evil now.<HB>
        <HB>
        &nbsp;&nbsp;&nbsp;You really shouldnt use it<HB>
        <HB>
        And that's the docs
        ");
    }

    // I don't know if this is valid syntax but we preserve stuff before non-code blocks like
    // `..deprecated ::`
    #[test]
    fn deprecated_prefix_gunk() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        wow this is some changes .. deprecated:: 1.2.3
            x = 2
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        **wow this is some changes Deprecated since version 1.2.3:**<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;x = 2
        ");
    }

    // We should not parse the contents of a markdown codefence
    #[test]
    fn explicit_markdown_block_with_ps1_contents() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func:

        ```python
        >>> thing.do_thing()
        wow it did the thing
        >>> thing.do_other_thing()
        it sure did the thing
        ```
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func:<HB>
        <HB>
        ```python
        >>> thing.do_thing()
        wow it did the thing
        >>> thing.do_other_thing()
        it sure did the thing
        ```
        ");
    }

    // We should not parse the contents of a markdown codefence
    #[test]
    fn explicit_markdown_block_with_underscore_contents_tick() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func:

        `````python
        x_y = thing_do();
        ``` # this should't close the fence!
        a_b = other_thing();
        `````
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func:<HB>
        <HB>
        `````python
        x_y = thing_do();
        ``` # this should't close the fence!
        a_b = other_thing();
        `````
        ");
    }

    // `~~~` also starts a markdown codefence
    #[test]
    fn explicit_markdown_block_with_underscore_contents_tilde() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func:

        ~~~~~python
        x_y = thing_do();
        ~~~ # this should't close the fence!
        a_b = other_thing();
        ~~~~~
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func:<HB>
        <HB>
        ~~~~~python
        x_y = thing_do();
        ~~~ # this should't close the fence!
        a_b = other_thing();
        ~~~~~
        ");
    }

    // If an explicit markdown codefence is indented, eat the indent so it renders
    // "the way the user expects" (as written this is basically invalid markdown,
    // but it's nice if we handle it anyway because it makes visual sense).
    #[test]
    fn explicit_markdown_block_with_indent_tick() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func...

        Returns:
            Some details
            `````python
            x_y = thing_do();
            ``` # this should't close the fence!
            a_b = other_thing();
            `````
            And so on.
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func...

        ## Returns
        Some details

        `````python
        x_y = thing_do();
        ``` # this should't close the fence!
        a_b = other_thing();
        `````<HB>
        And so on.
        ");
    }

    // If an explicit markdown codefence is indented, eat the indent so it renders
    // "the way the user expects" (as written this is basically invalid markdown,
    // but it's nice if we handle it anyway because it makes visual sense).
    #[test]
    fn explicit_markdown_block_with_indent_tilde() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func...

        Returns:
            Some details
            ~~~~~~python
            x_y = thing_do();
            ~~~ # this should't close the fence!
            a_b = other_thing();
            ~~~~~~
            And so on.
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func...

        ## Returns
        Some details

        ~~~~~~python
        x_y = thing_do();
        ~~~ # this should't close the fence!
        a_b = other_thing();
        ~~~~~~<HB>
        And so on.
        ");
    }

    // What do we do when we hit the end of the docstring with an unclosed markdown block?
    #[test]
    fn explicit_markdown_block_with_unclosed_fence_tick() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func:

        ````python
        x_y = thing_do();
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func:<HB>
        <HB>
        ````python
        x_y = thing_do();
        ````
        ");
    }

    // What do we do when we hit the end of the docstring with an unclosed markdown block?
    #[test]
    fn explicit_markdown_block_with_unclosed_fence_tilde() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func:

        ~~~~~python
        x_y = thing_do();
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func:<HB>
        <HB>
        ~~~~~python
        x_y = thing_do();
        ~~~~~
        ");
    }

    // Demonstration of where we're unreasonably lax about markdown block parsing.
    // It's fine to break this test, it's not particularly intentional behaviour.
    #[test]
    fn explicit_markdown_block_messy_corners_tick() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func:

                ``````we still think this is a codefence```
            x_y = thing_do();
        ```````````` and are sloppy as heck with indentation and closing shrugggg
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func:<HB>
        <HB>
        ``````we still think this is a codefence```
            x_y = thing_do();
        ```````````` and are sloppy as heck with indentation and closing shrugggg
        ");
    }

    // Demonstration of where we're unreasonably lax about markdown block parsing.
    // It's fine to break this test, it's not particularly intentional behaviour.
    #[test]
    fn explicit_markdown_block_messy_corners_tilde() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        My cool func:

                ~~~~~~we still think this is a codefence~~~
            x_y = thing_do();
        ~~~~~~~~~~~~~ and are sloppy as heck with indentation and closing shrugggg
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        My cool func:<HB>
        <HB>
        ~~~~~~we still think this is a codefence~~~
            x_y = thing_do();
        ~~~~~~~~~~~~~ and are sloppy as heck with indentation and closing shrugggg
        ");
    }

    // `.. code::` is a literal block and the `.. code::` should be deleted
    #[test]
    fn code_block() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Here's some code!

        .. code::
            def main() {
                print("hello world!")
            }
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Here's some code!<HB>
        <HB>
        <HB>
        ```````````python
            def main() {
                print("hello world!")
            }
        ```````````
        "#);
    }

    // `.. code:: rust` is a literal block with rust syntax highlighting
    #[test]
    fn code_block_lang() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Here's some Rust code!

        .. code:: rust
            fn main() {
                println!("hello world!");
            }
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Here's some Rust code!<HB>
        <HB>
        <HB>
        ```````````rust
            fn main() {
                println!("hello world!");
            }
        ```````````
        "#);
    }

    // I don't know if this is valid syntax but we preserve stuff before `..code ::`
    #[test]
    fn code_block_prefix_gunk() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        wow this is some code.. code:: abc
            x = 2
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        wow this is some code<HB>
        ```````````abc
            x = 2
        ```````````
        ");
    }

    // `.. asdgfhjkl-unknown::` is treated the same as `.. code::`
    #[test]
    fn unknown_block() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Here's some code!

        .. asdgfhjkl-unknown::
            fn main() {
                println!("hello world!");
            }
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Here's some code!<HB>
        <HB>
        <HB>
        ```````````python
            fn main() {
                println!("hello world!");
            }
        ```````````
        "#);
    }

    // `.. asdgfhjkl-unknown:: rust` is treated the same as `.. code:: rust`
    #[test]
    fn unknown_block_lang() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        Here's some Rust code!

        .. asdgfhjkl-unknown::   rust
            fn main() {
                print("hello world!")
            }
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @r#"
        Here's some Rust code!<HB>
        <HB>
        <HB>
        ```````````rust
            fn main() {
                print("hello world!")
            }
        ```````````
        "#);
    }

    // A nice doctest that is surrounded by prose
    #[test]
    fn doctest_simple() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description

        >>> thing.do_thing()
        wow it did the thing
        >>> thing.do_other_thing()
        it sure did the thing

        As you can see it did the thing!
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description<HB>
        <HB>
        ```````````python
        >>> thing.do_thing()
        wow it did the thing
        >>> thing.do_other_thing()
        it sure did the thing
        ```````````<HB>
        As you can see it did the thing!
        ");
    }

    // A nice doctest that is surrounded by prose with an indent
    #[test]
    fn doctest_simple_indent() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description

            >>> thing.do_thing()
            wow it did the thing
            >>> thing.do_other_thing()
            it sure did the thing

        As you can see it did the thing!
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description<HB>
        <HB>
        ```````````python
            >>> thing.do_thing()
            wow it did the thing
            >>> thing.do_other_thing()
            it sure did the thing
        ```````````<HB>
        As you can see it did the thing!
        ");
    }

    // A doctest that has nothing around it
    #[test]
    fn doctest_flush() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#">>> thing.do_thing()
        wow it did the thing
        >>> thing.do_other_thing()
        it sure did the thing"#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        ```````````python
        >>> thing.do_thing()
        wow it did the thing
        >>> thing.do_other_thing()
        it sure did the thing
        ```````````
        ");
    }

    // A doctest embedded in a literal block (it's just a literal block)
    #[test]
    fn literal_doctest() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description::

            >>> thing.do_thing()
            wow it did the thing
            >>> thing.do_other_thing()
            it sure did the thing

        As you can see it did the thing!
        "#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description:  <HB>
        ```````````python
            >>> thing.do_thing()
            wow it did the thing
            >>> thing.do_other_thing()
            it sure did the thing

        ```````````
        As you can see it did the thing!
        ");
    }

    #[test]
    fn doctest_indent_flush() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        And so you can see that
            >>> thing.do_thing()
            wow it did the thing
            >>> thing.do_other_thing()
            it sure did the thing"#;

        let docstring = Docstring::new(docstring.to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        And so you can see that<HB>
        ```````````python
            >>> thing.do_thing()
            wow it did the thing
            >>> thing.do_other_thing()
            it sure did the thing
        ```````````
        ");
    }

    #[test]
    fn test_google_style_parameter_documentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description.

        Args:
            param1 (str): The first parameter description
            param2 (int): The second parameter description
                This is a continuation of param2 description.
            param3: A parameter without type annotation

        Keyword Args:
            keyword_only (bool): Keyword-only parameter description

        Returns:
            str: The return value description

        Yields:
            int: The next value
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 4);
        assert_eq!(&param_docs["param1"], "The first parameter description");
        assert_eq!(
            &param_docs["param2"],
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(&param_docs["param3"], "A parameter without type annotation");
        assert_eq!(
            &param_docs["keyword_only"],
            "Keyword-only parameter description"
        );

        assert_snapshot!(docstring.render_plaintext(), @"
        This is a function description.

        Args:
            param1 (str): The first parameter description
            param2 (int): The second parameter description
                This is a continuation of param2 description.
            param3: A parameter without type annotation

        Keyword Args:
            keyword_only (bool): Keyword-only parameter description

        Returns:
            str: The return value description

        Yields:
            int: The next value
        ");

        assert_snapshot!(docstring.render_markdown(), @r"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter description

        **param2**: `int`<HB>
        The second parameter description<HB>
        This is a continuation of param2 description.

        **param3**<HB>
        A parameter without type annotation

        ## Keyword Arguments
        **keyword\_only**: `bool`<HB>
        Keyword-only parameter description

        ## Returns
        `str`<HB>
        The return value description

        ## Yields
        `int`<HB>
        The next value
        ");
    }

    #[test]
    fn google_markdown_renders_first_line_section() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = Docstring::new(
            "\
Args:
    value: Description.
    Aligned continuation.
    other: More."
                .to_owned(),
        );

        assert_eq!(
            docstring.parameter_documentation()["value"],
            "Description.\nAligned continuation."
        );

        assert_snapshot!(docstring.render_markdown(), @"
        ## Parameters
        **value**<HB>
        Description.<HB>
        Aligned continuation.

        **other**<HB>
        More.
        ");
    }

    #[test]
    fn google_markdown_renders_sections_shifted_by_decoded_newlines() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = Docstring::new(
            "\
A decoded newline follows:
This line starts at column zero.

    Keyword Args:
        shifted: Documentation in a shifted section.

    Returns:
        bool: Result."
                .to_owned(),
        );

        assert_eq!(
            docstring.parameter_documentation()["shifted"],
            "Documentation in a shifted section."
        );
        assert_snapshot!(docstring.render_markdown(), @"
        A decoded newline follows:<HB>
        This line starts at column zero.

        ## Keyword Arguments
        **shifted**<HB>
        Documentation in a shifted section.

        ## Returns
        `bool`<HB>
        Result.
        ");
    }

    #[test]
    fn google_markdown_renders_non_parameter_first_line_sections() {
        for (source, expected) in [
            (
                "Returns:\n    bool: Whether validation passed.\n    Additional details.",
                "## Returns\n`bool`  \nWhether validation passed.  \nAdditional details.",
            ),
            ("Yields:\n    The next item.", "## Yields\nThe next item."),
            (
                "Raises:\n    ValueError: If invalid.",
                "## Raises\n`ValueError`  \nIf invalid.",
            ),
            (
                "Raises:\n    Warning:\n        Emitted for legacy input.\n    Error: Generic failure.",
                "## Raises\n`Warning`  \nEmitted for legacy input.\n\n`Error`  \nGeneric failure.",
            ),
            (
                "Attributes:\n    name (str): Display name.",
                "## Attributes\n**name**: `str`  \nDisplay name.",
            ),
            (
                "Other Parameters:\n    timeout (float): Maximum wait in seconds.",
                "## Other Parameters\n**timeout**: `float`  \nMaximum wait in seconds.",
            ),
        ] {
            assert_eq!(
                Docstring::new(source.to_owned()).render_markdown(),
                expected
            );
        }
    }

    #[test]
    fn google_sections_render_edge_cases() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = Docstring::new(
            "\
Attributes:
    name (str): Display name.
    coords (tuple(int, int)): Coordinate pair.
    callback (Callable(int, str)): Converts raw values.
        if name:
            return name

Raises:
    ValueError: If invalid."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        ## Attributes
        **name**: `str`<HB>
        Display name.

        **coords**: `tuple(int, int)`<HB>
        Coordinate pair.

        **callback**: `Callable(int, str)`<HB>
        Converts raw values.<HB>
        if name:<HB>
            return name

        ## Raises
        `ValueError`<HB>
        If invalid.
        ");

        let docstring = Docstring::new(
            "\
Summary.

Args:
    value: Description.
        ```python
        Args:
            nested: Still code.
        Returns:
            Still code.
        ```
    url (Literal[\"http://\"]): URL."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @r#"
        Summary.

        ## Parameters
        **value**<HB>
        Description.

        ```python
        Args:
            nested: Still code.
        Returns:
            Still code.
        ```

        **url**: `Literal["http://"]`<HB>
        URL.
        "#);

        let docstring = Docstring::new(
            "\
Summary.

Returns:
    Literal[\"http://\"]: First paragraph.

    Second paragraph."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @r#"
        Summary.

        ## Returns
        `Literal["http://"]`<HB>
        First paragraph.

        Second paragraph.
        "#);

        let docstring = Docstring::new(
            "\
Summary.

Returns:
    str | None: Optional value.

Yields:
    :obj:`list` of :obj:`str`: Result chunks."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        Summary.

        ## Returns
        `str | None`<HB>
        Optional value.

        ## Yields
        `` :obj:`list` of :obj:`str` ``<HB>
        Result chunks.
        ");

        let docstring = Docstring::new(
            "\
Summary.

Args:
    value: Description.
>>> value
42"
            .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        Summary.

        ## Parameters
        **value**<HB>
        Description.

        ```````````python
        >>> value
        42
        ```````````
        ");

        let docstring = Docstring::new(
            "\
Summary.

Returns:
    https://example.com: more details."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        Summary.

        ## Returns
        https://example.com: more details.
        ");

        let docstring = Docstring::new(
            "\
Summary.

Returns:
    str:Description without whitespace."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        Summary.

        ## Returns
        `str`<HB>
        Description without whitespace.
        ");

        let docstring = Docstring::new(
            "\
Args:
    : Missing name."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        Args:<HB>
        : Missing name.
    ");
    }

    #[test]
    fn google_style_parameter_documentation_keeps_prose_with_colons() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description.

        Args:
            param1 (str): The first parameter description.
            For example: pass an absolute path.
            *args: Extra positional arguments.
            **kwargs: Extra keyword arguments.
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            &param_docs["param1"],
            "The first parameter description.\nFor example: pass an absolute path."
        );
        assert_eq!(&param_docs["*args"], "Extra positional arguments.");
        assert_eq!(&param_docs["**kwargs"], "Extra keyword arguments.");

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description.<HB>
        <HB>
        Args:<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;param1 (str): The first parameter description.<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;For example: pass an absolute path.<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;*args: Extra positional arguments.<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;**kwargs: Extra keyword arguments.
        ");
    }

    #[test]
    fn test_numpy_style_parameter_documentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description.

        Parameters
        ----------
        param1 : str
            The first parameter description
        param2, param4 : int
            The shared parameter description

            This is a second paragraph.
            This is a continuation of the shared description.
        param3
            A parameter without type annotation
        *args : object
            Extra positional arguments
        **kwargs : object
            Extra keyword arguments
        options.mode : str
            Nested field documentation
        π : int
            A Unicode parameter
        a1, a2, ... : sequence of array_like
            Arrays to combine
        \*escaped_args : object
            Escaped positional arguments
        \**escaped_kwargs : object
            Escaped keyword arguments
        override_repr: callable, optional
            Replacement representation function
        formats, names :
        undocumented
        copy : bool
            Whether to copy the input

        Other Parameters
        ----------------
        kw_only : str, optional
            A less commonly used keyword-only parameter

        Other Params
        ------------
        alias_only : int
            Parameter under the abbreviated heading

        Returns
        -------
        str
            The return value description

        Yields
        ------
        int
            The next value
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 16);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The shared parameter description\n\nThis is a second paragraph.\nThis is a continuation of the shared description."
        );
        assert_eq!(
            param_docs.get("param4").expect("param4 should exist"),
            "The shared parameter description\n\nThis is a second paragraph.\nThis is a continuation of the shared description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );
        assert_eq!(
            param_docs.get("*args").expect("*args should exist"),
            "Extra positional arguments"
        );
        assert_eq!(
            param_docs.get("**kwargs").expect("**kwargs should exist"),
            "Extra keyword arguments"
        );
        assert!(!param_docs.contains_key("options"));
        assert_eq!(
            param_docs
                .get("options.mode")
                .expect("options.mode should exist"),
            "Nested field documentation"
        );
        assert_eq!(
            param_docs.get("π").expect("π should exist"),
            "A Unicode parameter"
        );
        assert_eq!(
            param_docs.get("a1").expect("a1 should exist"),
            "Arrays to combine"
        );
        assert_eq!(
            param_docs.get("a2").expect("a2 should exist"),
            "Arrays to combine"
        );
        assert_eq!(
            param_docs
                .get("*escaped_args")
                .expect("*escaped_args should exist"),
            "Escaped positional arguments"
        );
        assert_eq!(
            param_docs
                .get("**escaped_kwargs")
                .expect("**escaped_kwargs should exist"),
            "Escaped keyword arguments"
        );
        assert_eq!(
            param_docs
                .get("override_repr")
                .expect("override_repr should exist"),
            "Replacement representation function"
        );
        assert_eq!(
            param_docs.get("copy").expect("copy should exist"),
            "Whether to copy the input"
        );
        assert_eq!(
            param_docs.get("kw_only").expect("kw_only should exist"),
            "A less commonly used keyword-only parameter"
        );
        assert_eq!(
            param_docs
                .get("alias_only")
                .expect("alias_only should exist"),
            "Parameter under the abbreviated heading"
        );

        assert_snapshot!(docstring.render_plaintext(), @r"
        This is a function description.

        Parameters
        ----------
        param1 : str
            The first parameter description
        param2, param4 : int
            The shared parameter description

            This is a second paragraph.
            This is a continuation of the shared description.
        param3
            A parameter without type annotation
        *args : object
            Extra positional arguments
        **kwargs : object
            Extra keyword arguments
        options.mode : str
            Nested field documentation
        π : int
            A Unicode parameter
        a1, a2, ... : sequence of array_like
            Arrays to combine
        \*escaped_args : object
            Escaped positional arguments
        \**escaped_kwargs : object
            Escaped keyword arguments
        override_repr: callable, optional
            Replacement representation function
        formats, names :
        undocumented
        copy : bool
            Whether to copy the input

        Other Parameters
        ----------------
        kw_only : str, optional
            A less commonly used keyword-only parameter

        Other Params
        ------------
        alias_only : int
            Parameter under the abbreviated heading

        Returns
        -------
        str
            The return value description

        Yields
        ------
        int
            The next value
        ");

        assert_snapshot!(docstring.render_markdown(), @r"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter description

        **param2, param4**: `int`<HB>
        The shared parameter description

        This is a second paragraph.<HB>
        This is a continuation of the shared description.

        **param3**<HB>
        A parameter without type annotation

        **\*args**: `object`<HB>
        Extra positional arguments

        **\*\*kwargs**: `object`<HB>
        Extra keyword arguments

        **options.mode**: `str`<HB>
        Nested field documentation

        **π**: `int`<HB>
        A Unicode parameter

        **a1, a2, ...**: `sequence of array_like`<HB>
        Arrays to combine

        **\*escaped\_args**: `object`<HB>
        Escaped positional arguments

        **\*\*escaped\_kwargs**: `object`<HB>
        Escaped keyword arguments

        **override\_repr**: `callable, optional`<HB>
        Replacement representation function

        **formats, names**

        **undocumented**

        **copy**: `bool`<HB>
        Whether to copy the input

        ## Other Parameters
        **kw\_only**: `str, optional`<HB>
        A less commonly used keyword-only parameter

        ## Other Parameters
        **alias\_only**: `int`<HB>
        Parameter under the abbreviated heading

        ## Returns
        `str`<HB>
        The return value description

        ## Yields
        `int`<HB>
        The next value
        ");
    }

    #[test]
    fn numpy_sections_render_edge_cases() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = Docstring::new(
            "\
Attributes
----------
name : str
    Display name.
Note: deprecated

Raises
------
ValueError
    If invalid.
TypeError : If wrong type.
RuntimeError : If unavailable.
    Retry later.
This paragraph is not an exception."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        ## Attributes
        **name**: `str`<HB>
        Display name.

        Note: deprecated

        ## Raises
        `ValueError`<HB>
        If invalid.

        `TypeError`<HB>
        If wrong type.

        `RuntimeError`<HB>
        If unavailable.<HB>
        Retry later.

        This paragraph is not an exception.
        ");

        let docstring = Docstring::new(
            "\
Parameters
----------
name : str
    Display name.
    if name:
        return name
Note: this paragraph is not a parameter.

Returns
-------
str
    Display result.
Note: deprecated"
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        ## Parameters
        **name**: `str`<HB>
        Display name.<HB>
        if name:<HB>
            return name

        Note: this paragraph is not a parameter.

        ## Returns
        `str`<HB>
        Display result.

        Note: deprecated
        ");

        let docstring = Docstring::new(
            "\
Parameters
----------
value : int
    Example.
    >>> value
    1

Returns
-------
bool
    Done."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        ## Parameters
        **value**: `int`<HB>
        Example.

        ```````````python
        >>> value
        1
        ```````````

        ## Returns
        `bool`<HB>
        Done.
        ");

        let docstring = Docstring::new(
            "\
Parameters
----------
value : int
    ```python
    first()

print(\"still code\")
    ```

Returns
-------
bool
    Done."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @r#"
        Parameters<HB>
        ----------<HB>
        value : int<HB>
        ```python
            first()

        print("still code")
        ```

        ## Returns
        `bool`<HB>
        Done.
        "#);

        let mut source = "\
Parameters
----------
first : int
    First.
"
        .to_owned();
        // Keep this large enough that accidentally rescanning every remaining blank suffix is
        // noticeable, without spelling the generated whitespace inline.
        source.push_str(&"\n".repeat(10_000));
        source.push_str(
            "\
second : int
    Second.",
        );
        let docstring = Docstring::new(source);

        assert_eq!(docstring.parameter_documentation()["second"], "Second.");

        let docstring = Docstring::new(
            "\
Returns
-------
Literal[\"header : value\", \"http://\"]
    First paragraph.

    Second paragraph."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @r#"
        ## Returns
        `Literal["header : value", "http://"]`<HB>
        First paragraph.

        Second paragraph.
        "#);

        let docstring = Docstring::new(
            "\
Returns
-------
ctypes.cdll[libpath] : library object
    A ctypes library object."
                .to_owned(),
        );

        assert_snapshot!(docstring.render_markdown(), @"
        Returns<HB>
        -------<HB>
        ctypes.cdll[libpath] : library object<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;A ctypes library object.
        ");

        let docstring = Docstring::new(
            r#"
        Parameters
        ----------
        value : str
            First documentation.
        value : str
            Replacement documentation.
        "#
            .to_owned(),
        );

        assert_eq!(
            docstring.parameter_documentation()["value"],
            "Replacement documentation."
        );
    }

    #[test]
    fn numpy_parameter_documentation_accepts_compact_separators() {
        let docstring = Docstring::new(
            "\
Parameters
----------
matrix: scipy.sparse array
    Sparse adjacency matrix.
args:
    Additional arguments.
*values:
    Additional values.
Note: deprecated

Returns
-------
bool
    Whether processing succeeded."
                .to_owned(),
        );

        let parameter_documentation = docstring.parameter_documentation();
        assert_eq!(parameter_documentation.len(), 3);
        assert_eq!(
            parameter_documentation["matrix"],
            "Sparse adjacency matrix."
        );
        assert_eq!(parameter_documentation["args"], "Additional arguments.");
        assert_eq!(parameter_documentation["*values"], "Additional values.");
        assert!(!parameter_documentation.contains_key("Note"));
    }

    #[test]
    fn numpy_parameter_documentation_continues_after_undocumented_compact_items() {
        let docstring = Docstring::new(
            "\
Parameters
----------
G: Graph
beta : float
    Useful documentation."
                .to_owned(),
        );

        let parameter_documentation = docstring.parameter_documentation();
        assert_eq!(parameter_documentation.len(), 1);
        assert_eq!(parameter_documentation["beta"], "Useful documentation.");
    }

    #[test]
    fn numpy_parameter_documentation_skips_leading_section_prose() {
        let docstring = Docstring::new(
            "\
Parameters
----------
Either x or y must be provided.

beta : float
    Useful documentation."
                .to_owned(),
        );

        let parameter_documentation = docstring.parameter_documentation();
        assert_eq!(parameter_documentation.len(), 1);
        assert_eq!(parameter_documentation["beta"], "Useful documentation.");
    }

    #[test]
    fn numpy_parameter_documentation_ignores_items_nested_in_section_preambles() {
        let docstring = Docstring::new(
            "\
Parameters
----------
Choose one of the following.
    nested : int
        Example-only text.
beta : float
    Useful documentation."
                .to_owned(),
        );

        let parameter_documentation = docstring.parameter_documentation();
        assert_eq!(parameter_documentation.len(), 1);
        assert_eq!(parameter_documentation["beta"], "Useful documentation.");
    }

    #[test]
    fn numpy_parameter_documentation_ignores_sections_in_containers() {
        for raw in [
            "\
Summary.

- Example data:
    Parameters
    ----------
    nested : int
        Not parameter documentation.",
            "\
Summary.

Examples:
    Parameters
    ----------
    nested : int
        Not parameter documentation.",
        ] {
            assert!(
                Docstring::new(raw.to_owned())
                    .parameter_documentation()
                    .is_empty(),
                "{raw}"
            );
        }

        let docstring = Docstring::new(
            "\
:param value: Example input.

    Parameters
    ----------
    nested : int
        Not parameter documentation.
:param other: Other input."
                .to_owned(),
        );
        let parameter_documentation = docstring.parameter_documentation();
        assert_eq!(parameter_documentation.len(), 2);
        assert!(parameter_documentation.contains_key("value"));
        assert!(parameter_documentation.contains_key("other"));
        assert!(!parameter_documentation.contains_key("nested"));
    }

    #[test]
    fn numpy_parameter_documentation_ignores_sections_nested_in_underlined_containers() {
        let docstring = Docstring::new(
            "\
Examples
--------
    Parameters
    ----------
    nested : int
        Not parameter documentation.

Notes
-----
More details.

Parameters
----------
value : int
    Parameter documentation."
                .to_owned(),
        );

        let parameter_documentation = docstring.parameter_documentation();
        assert_eq!(parameter_documentation.len(), 1);
        assert_eq!(parameter_documentation["value"], "Parameter documentation.");
        assert!(!parameter_documentation.contains_key("nested"));
    }

    #[test]
    fn numpy_parameter_documentation_accepts_shifted_top_level_sections() {
        let docstring = Docstring::new(
            "\
A decoded newline follows:
This line starts at column zero.

    Parameters
    ----------
    shifted : int
        Documentation in a shifted section.

    Returns
    -------
    bool
        Result."
                .to_owned(),
        );

        assert_eq!(
            docstring.parameter_documentation()["shifted"],
            "Documentation in a shifted section."
        );
    }

    #[test]
    fn test_pep257_style_parameter_documentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"Insert an entry into the list of warnings filters (at the front).

        'param1' -- The first parameter description
        'param2' -- The second parameter description
                    This is a continuation of param2 description.
        'param3' -- A parameter without type annotation

        >>> print repr(foo.__doc__)
        '\n    This is the second line of the docstring.\n    '
        >>> foo.__doc__.splitlines()
        ['', '    This is the second line of the docstring.', '    ']
        >>> trim(foo.__doc__)
        'This is the second line of the docstring.'
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();
        assert!(param_docs.is_empty());

        assert_snapshot!(docstring.render_plaintext(), @r"
        Insert an entry into the list of warnings filters (at the front).

        'param1' -- The first parameter description
        'param2' -- The second parameter description
                    This is a continuation of param2 description.
        'param3' -- A parameter without type annotation

        >>> print repr(foo.__doc__)
        '\n    This is the second line of the docstring.\n    '
        >>> foo.__doc__.splitlines()
        ['', '    This is the second line of the docstring.', '    ']
        >>> trim(foo.__doc__)
        'This is the second line of the docstring.'
        ");

        assert_snapshot!(docstring.render_markdown(), @r"
        Insert an entry into the list of warnings filters (at the front).<HB>
        <HB>
        'param1' -- The first parameter description<HB>
        'param2' -- The second parameter description<HB>
        &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;This is a continuation of param2 description.<HB>
        'param3' -- A parameter without type annotation<HB>
        <HB>
        ```````````python
        >>> print repr(foo.__doc__)
        '\n    This is the second line of the docstring.\n    '
        >>> foo.__doc__.splitlines()
        ['', '    This is the second line of the docstring.', '    ']
        >>> trim(foo.__doc__)
        'This is the second line of the docstring.'
        ```````````
        ");
    }

    #[test]
    fn test_no_parameter_documentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a simple function description without parameter documentation.
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();
        assert!(param_docs.is_empty());

        assert_snapshot!(docstring.render_plaintext(), @"This is a simple function description without parameter documentation.");

        assert_snapshot!(docstring.render_markdown(), @"This is a simple function description without parameter documentation.");
    }

    #[test]
    fn test_mixed_style_parameter_documentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description.

        Args:
            param1 (str): Google-style parameter
            param2 (int): Another Google-style parameter

        Parameters
        ----------
        param3 : bool
            NumPy-style parameter
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "Google-style parameter"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "Another Google-style parameter"
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "NumPy-style parameter"
        );

        assert_snapshot!(docstring.render_plaintext(), @"
        This is a function description.

        Args:
            param1 (str): Google-style parameter
            param2 (int): Another Google-style parameter

        Parameters
        ----------
        param3 : bool
            NumPy-style parameter
        ");

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        Google-style parameter

        **param2**: `int`<HB>
        Another Google-style parameter

        ## Parameters
        **param3**: `bool`<HB>
        NumPy-style parameter
        ");

        let docstring = Docstring::new(
            r#"
        Args:
            value: Google-style parameter

        Parameters
        ----------
        value : str
            NumPy-style parameter
        "#
            .to_owned(),
        );

        assert_eq!(
            docstring.parameter_documentation()["value"],
            "NumPy-style parameter"
        );
    }

    /// PEP 257 trimming removes indentation from a docstring's first physical
    /// line. If a field starts there, continuation indentation is ambiguous, so
    /// leave the normalized text unchanged rather than guessing. Starting the
    /// field after the opening newline or a summary makes the intent unambiguous.
    #[test]
    fn rest_markdown_does_not_special_case_first_line_field_continuation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = Docstring::new(":param value: First line.\n    Second line.".to_owned());

        assert_snapshot!(docstring.render_markdown(), @"
        ## Parameters
        **value**<HB>
        First line.

        Second line.
        ");
    }

    #[test]
    fn test_rest_style_parameter_documentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description.

        :param str param1: The first parameter description
        :param int param2: The second parameter description
            This is a continuation of param2 description.
        :param param3: A parameter without type annotation
        :returns: The return value description
        :rtype: str
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );

        assert_snapshot!(docstring.render_plaintext(), @"
        This is a function description.

        :param str param1: The first parameter description
        :param int param2: The second parameter description
            This is a continuation of param2 description.
        :param param3: A parameter without type annotation
        :returns: The return value description
        :rtype: str
        ");

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter description

        **param2**: `int`<HB>
        The second parameter description<HB>
        This is a continuation of param2 description.

        **param3**<HB>
        A parameter without type annotation

        ## Returns
        `str`<HB>
        The return value description
        ");
    }

    #[test]
    fn test_mixed_style_with_rest_parameter_documentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description.

        Args:
            param1 (str): Google-style parameter
            param2 (int): Google-style duplicate parameter

        :param int param2: reST-style parameter
        :param param3: Another reST-style parameter

        Parameters
        ----------
        param3 : str
            NumPy-style duplicate parameter
        param4 : bool
            NumPy-style parameter
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 4);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "Google-style parameter"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "reST-style parameter"
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "Another reST-style parameter"
        );
        assert_eq!(
            param_docs.get("param4").expect("param4 should exist"),
            "NumPy-style parameter"
        );

        assert_snapshot!(docstring.render_plaintext(), @"
        This is a function description.

        Args:
            param1 (str): Google-style parameter
            param2 (int): Google-style duplicate parameter

        :param int param2: reST-style parameter
        :param param3: Another reST-style parameter

        Parameters
        ----------
        param3 : str
            NumPy-style duplicate parameter
        param4 : bool
            NumPy-style parameter
        ");

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        Google-style parameter

        **param2**: `int`<HB>
        Google-style duplicate parameter

        ## Parameters
        **param2**: `int`<HB>
        reST-style parameter

        **param3**<HB>
        Another reST-style parameter

        ## Parameters
        **param3**: `str`<HB>
        NumPy-style duplicate parameter

        **param4**: `bool`<HB>
        NumPy-style parameter
        ");
    }

    #[test]
    fn test_numpy_style_with_different_indentation() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = r#"
        This is a function description.

        Parameters
        ----------
        param1 : str
            The first parameter description
        param2 : int
            The second parameter description
            This is a continuation of param2 description.
        param3
            A parameter without type annotation

        Returns
        -------
        str
            The return value description
        "#;

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );

        assert_snapshot!(docstring.render_plaintext(), @"
        This is a function description.

        Parameters
        ----------
        param1 : str
            The first parameter description
        param2 : int
            The second parameter description
            This is a continuation of param2 description.
        param3
            A parameter without type annotation

        Returns
        -------
        str
            The return value description
        ");

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter description

        **param2**: `int`<HB>
        The second parameter description<HB>
        This is a continuation of param2 description.

        **param3**<HB>
        A parameter without type annotation

        ## Returns
        `str`<HB>
        The return value description
        ");
    }

    #[test]
    fn test_numpy_style_with_tabs_and_mixed_indentation() {
        let _snap = bind_docstring_snapshot_filters();
        // Using raw strings to avoid tab/space conversion issues in the test
        let docstring = "
        This is a function description.

        Parameters
        ----------
\tparam1 : str
\t\tThe first parameter description
\tparam2 : int
\t\tThe second parameter description
\t\tThis is a continuation of param2 description.
\tparam3
\t\tA parameter without type annotation
        ";

        let docstring = Docstring::new(docstring.to_owned());
        let param_docs = docstring.parameter_documentation();

        assert_eq!(param_docs.len(), 3);
        assert_eq!(
            param_docs.get("param1").expect("param1 should exist"),
            "The first parameter description"
        );
        assert_eq!(
            param_docs.get("param2").expect("param2 should exist"),
            "The second parameter description\nThis is a continuation of param2 description."
        );
        assert_eq!(
            param_docs.get("param3").expect("param3 should exist"),
            "A parameter without type annotation"
        );

        assert_snapshot!(docstring.render_plaintext(), @"
        This is a function description.

        Parameters
        ----------
        param1 : str
                The first parameter description
        param2 : int
                The second parameter description
                This is a continuation of param2 description.
        param3
                A parameter without type annotation
        ");

        assert_snapshot!(docstring.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter description

        **param2**: `int`<HB>
        The second parameter description<HB>
        This is a continuation of param2 description.

        **param3**<HB>
        A parameter without type annotation
        ");
    }

    #[test]
    fn test_universal_newlines() {
        let _snap = bind_docstring_snapshot_filters();
        // Test with Windows-style line endings (\r\n)
        let docstring_windows = "This is a function description.\r\n\r\nArgs:\r\n    param1 (str): The first parameter\r\n    param2 (int): The second parameter\r\n";

        // Test with old Mac-style line endings (\r)
        let docstring_mac = "This is a function description.\r\rArgs:\r    param1 (str): The first parameter\r    param2 (int): The second parameter\r";

        // Test with Unix-style line endings (\n) - should work the same
        let docstring_unix = "This is a function description.\n\nArgs:\n    param1 (str): The first parameter\n    param2 (int): The second parameter\n";

        let docstring_windows = Docstring::new(docstring_windows.to_owned());
        let docstring_mac = Docstring::new(docstring_mac.to_owned());
        let docstring_unix = Docstring::new(docstring_unix.to_owned());

        let param_docs_windows = docstring_windows.parameter_documentation();
        let param_docs_mac = docstring_mac.parameter_documentation();
        let param_docs_unix = docstring_unix.parameter_documentation();

        // All should produce the same results
        assert_eq!(param_docs_windows.len(), 2);
        assert_eq!(param_docs_mac.len(), 2);
        assert_eq!(param_docs_unix.len(), 2);

        assert_eq!(
            param_docs_windows.get("param1"),
            Some(&"The first parameter".to_string())
        );
        assert_eq!(
            param_docs_mac.get("param1"),
            Some(&"The first parameter".to_string())
        );
        assert_eq!(
            param_docs_unix.get("param1"),
            Some(&"The first parameter".to_string())
        );

        assert_snapshot!(docstring_windows.render_plaintext(), @"
        This is a function description.

        Args:
            param1 (str): The first parameter
            param2 (int): The second parameter
        ");

        assert_snapshot!(docstring_windows.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter

        **param2**: `int`<HB>
        The second parameter
        ");

        assert_snapshot!(docstring_mac.render_plaintext(), @"
        This is a function description.

        Args:
            param1 (str): The first parameter
            param2 (int): The second parameter
        ");

        assert_snapshot!(docstring_mac.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter

        **param2**: `int`<HB>
        The second parameter
        ");

        assert_snapshot!(docstring_unix.render_plaintext(), @"
        This is a function description.

        Args:
            param1 (str): The first parameter
            param2 (int): The second parameter
        ");

        assert_snapshot!(docstring_unix.render_markdown(), @"
        This is a function description.

        ## Parameters
        **param1**: `str`<HB>
        The first parameter

        **param2**: `int`<HB>
        The second parameter
        ");
    }

    // Regression test: a doctest followed by a literal block with blank lines inside.
    // Previously, in_doctest wasn't reset when ending a doctest, so a blank line inside
    // a subsequent literal block would incorrectly end the literal block early.
    // See: https://github.com/astral-sh/ty/issues/2497
    #[test]
    fn doctest_then_literal_block_with_blank_lines() {
        let _snap = bind_docstring_snapshot_filters();
        let docstring = Docstring::new(
            "\
Example:

>>> print(\"hello\")
hello

Code example::

    def foo():
        pass

    def bar():
        pass

Done.
"
            .to_owned(),
        );

        // The blank line between foo() and bar() should be preserved inside the code block,
        // NOT cause the code block to end early with bar() rendered as regular text.
        assert_snapshot!(docstring.render_markdown(), @r#"
        Example:<HB>
        <HB>
        ```````````python
        >>> print("hello")
        hello
        ```````````<HB>
        Code example:  <HB>
        ```````````python
            def foo():
                pass

            def bar():
                pass

        ```````````
        Done.
        "#);
    }
}
