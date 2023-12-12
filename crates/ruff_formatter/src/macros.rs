/// Constructs the parameters for other formatting macros.
///
/// This macro functions by taking a list of objects implementing [`crate::Format`]. It will canonicalize the
/// arguments into a single type.
///
/// This macro produces a value of type [`crate::Arguments`]. This value can be passed to
/// the macros within [crate]. All other formatting macros ([`format!`](crate::format!),
/// [`write!`](crate::write!)) are proxied through this one. This macro avoids heap allocations.
///
/// You can use the [`Arguments`] value that `format_args!` returns in  `Format` contexts
/// as seen below.
///
/// ```rust
/// use ruff_formatter::{SimpleFormatContext, format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(SimpleFormatContext::default(), [
///     format_args!(token("Hello World"))
/// ])?;
///
/// assert_eq!("Hello World", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
///
/// [`Format`]: crate::Format
/// [`Arguments`]: crate::Arguments
#[macro_export]
macro_rules! format_args {
    ($($value:expr),+ $(,)?) => {
        $crate::Arguments::new(&[
            $(
                $crate::Argument::new(&$value)
            ),+
        ])
    }
}

/// Writes formatted data into a buffer.
///
/// This macro accepts a 'buffer' and a list of format arguments. Each argument will be formatted
/// and the result will be passed to the buffer. The writer may be any value with a `write_fmt` method;
/// generally this comes from an implementation of the [`crate::Buffer`] trait.
///
/// # Examples
///
/// ```rust
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{Buffer, FormatState, SimpleFormatContext, VecBuffer, write};
///
/// # fn main() -> FormatResult<()> {
/// let mut state = FormatState::new(SimpleFormatContext::default());
/// let mut buffer = VecBuffer::new(&mut state);
/// write!(&mut buffer, [token("Hello"), space()])?;
/// write!(&mut buffer, [token("World")])?;
///
/// assert_eq!(
///     buffer.into_vec(),
///     vec![
///         FormatElement::Token { text: "Hello" },
///         FormatElement::Space,
///         FormatElement::Token { text: "World" },
///     ]
///  );
/// #  Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! write {
    ($dst:expr, [$($arg:expr),+ $(,)?]) => {{
        let result = $dst.write_fmt($crate::format_args!($($arg),+));
        result
    }}
}

/// Writes formatted data into the given buffer and prints all written elements for a quick and dirty debugging.
///
/// An example:
///
/// ```rust
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{FormatState, VecBuffer};
///
/// # fn main() -> FormatResult<()> {
/// let mut state = FormatState::new(SimpleFormatContext::default());
/// let mut buffer = VecBuffer::new(&mut state);
///
/// dbg_write!(buffer, [token("Hello")])?;
/// // ^-- prints: [src/main.rs:7][0] = StaticToken("Hello")
///
/// assert_eq!(buffer.into_vec(), vec![FormatElement::Token { text: "Hello" }]);
/// # Ok(())
/// # }
/// ```
///
/// Note that the macro is intended as debugging tool and therefore you should avoid having
/// uses of it in version control for long periods (other than in tests and similar). Format output
/// from production code is better done with `[write!]`
#[macro_export]
macro_rules! dbg_write {
    ($dst:expr, [$($arg:expr),+ $(,)?]) => {{
        use $crate::BufferExtensions;
        let mut count = 0;
        let mut inspect = $dst.inspect(|element: &FormatElement| {
            std::eprintln!(
                "[{}:{}][{}] = {element:#?}",
                std::file!(), std::line!(), count
            );
            count += 1;
        });
        let result = inspect.write_fmt($crate::format_args!($($arg),+));
        result
    }}
}

/// Creates the Format IR for a value.
///
/// The first argument `format!` receives is the [`crate::FormatContext`] that specify how elements must be formatted.
/// Additional parameters passed get formatted by using their [`crate::Format`] implementation.
///
///
/// ## Examples
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::format;
///
/// let formatted = format!(SimpleFormatContext::default(), [token("("), token("a"), token(")")]).unwrap();
///
/// assert_eq!(
///     formatted.into_document(),
///     Document::from(vec![
///         FormatElement::Token { text: "(" },
///         FormatElement::Token { text: "a" },
///         FormatElement::Token { text: ")" },
///     ])
/// );
/// ```
#[macro_export]
macro_rules! format {
    ($context:expr, [$($arg:expr),+ $(,)?]) => {{
        ($crate::format($context, $crate::format_args!($($arg),+)))
    }}
}

/// Provides multiple different alternatives and the printer picks the first one that fits.
/// Use this as last resort because it requires that the printer must try all variants in the worst case.
/// The passed variants must be in the following order:
/// - First: The variant that takes up most space horizontally
/// - Last: The variant that takes up the least space horizontally by splitting the content over multiple lines.
///
/// ## Examples
///
/// ```
/// use ruff_formatter::{Formatted, LineWidth, format, format_args, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(
///     SimpleFormatContext::default(),
///     [
///         token("aVeryLongIdentifier"),
///         best_fitting!(
///             // Everything fits on a single line
///             format_args!(
///                 token("("),
///                 group(&format_args![
///                     token("["),
///                         soft_block_indent(&format_args![
///                         token("1,"),
///                         soft_line_break_or_space(),
///                         token("2,"),
///                         soft_line_break_or_space(),
///                         token("3"),
///                     ]),
///                     token("]")
///                 ]),
///                 token(")")
///             ),
///
///             // Breaks after `[`, but prints all elements on a single line
///             format_args!(
///                 token("("),
///                 token("["),
///                 block_indent(&token("1, 2, 3")),
///                 token("]"),
///                 token(")"),
///             ),
///
///             // Breaks after `[` and prints each element on a single line
///             format_args!(
///                 token("("),
///                 block_indent(&format_args![
///                     token("["),
///                     block_indent(&format_args![
///                         token("1,"),
///                         hard_line_break(),
///                         token("2,"),
///                         hard_line_break(),
///                         token("3"),
///                     ]),
///                     token("]"),
///                 ]),
///                 token(")")
///             )
///         )
///     ]
/// )?;
///
/// let document = formatted.into_document();
///
/// // Takes the first variant if everything fits on a single line
/// assert_eq!(
///     "aVeryLongIdentifier([1, 2, 3])",
///     Formatted::new(document.clone(), SimpleFormatContext::default())
///         .print()?
///         .as_code()
/// );
///
/// // It takes the second if the first variant doesn't fit on a single line. The second variant
/// // has some additional line breaks to make sure inner groups don't break
/// assert_eq!(
///     "aVeryLongIdentifier([\n\t1, 2, 3\n])",
///     Formatted::new(document.clone(), SimpleFormatContext::new(SimpleFormatOptions { line_width: 21.try_into().unwrap(), ..SimpleFormatOptions::default() }))
///         .print()?
///         .as_code()
/// );
///
/// // Prints the last option as last resort
/// assert_eq!(
///     "aVeryLongIdentifier(\n\t[\n\t\t1,\n\t\t2,\n\t\t3\n\t]\n)",
///     Formatted::new(document.clone(), SimpleFormatContext::new(SimpleFormatOptions { line_width: 20.try_into().unwrap(), ..SimpleFormatOptions::default() }))
///         .print()?
///         .as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// ### Enclosing group with `should_expand: true`
///
/// ```
/// use ruff_formatter::{Formatted, LineWidth, format, format_args, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(
///     SimpleFormatContext::default(),
///     [
///         best_fitting!(
///             // Prints the method call on the line but breaks the array.
///             format_args!(
///                 token("expect(a).toMatch("),
///                 group(&format_args![
///                     token("["),
///                     soft_block_indent(&format_args![
///                         token("1,"),
///                         soft_line_break_or_space(),
///                         token("2,"),
///                         soft_line_break_or_space(),
///                         token("3"),
///                     ]),
///                     token("]")
///                 ]).should_expand(true),
///                 token(")")
///             ),
///
///             // Breaks after `(`
///            format_args!(
///                 token("expect(a).toMatch("),
///                 group(&soft_block_indent(
///                     &group(&format_args![
///                         token("["),
///                         soft_block_indent(&format_args![
///                             token("1,"),
///                             soft_line_break_or_space(),
///                             token("2,"),
///                             soft_line_break_or_space(),
///                             token("3"),
///                         ]),
///                         token("]")
///                     ]).should_expand(true),
///                 )).should_expand(true),
///                 token(")")
///             ),
///         )
///     ]
/// )?;
///
/// let document = formatted.into_document();
///
/// assert_eq!(
///     "expect(a).toMatch([\n\t1,\n\t2,\n\t3\n])",
///     Formatted::new(document.clone(), SimpleFormatContext::default())
///         .print()?
///         .as_code()
/// );
///
/// # Ok(())
/// # }
/// ```
///
/// The first variant fits because all its content up to the first line break fit on the line without exceeding
/// the configured print width.
///
/// ## Complexity
/// Be mindful of using this IR element as it has a considerable performance penalty:
/// - There are multiple representation for the same content. This results in increased memory usage
///   and traversal time in the printer.
/// - The worst case complexity is that the printer tires each variant. This can result in quadratic
///   complexity if used in nested structures.
///
/// ## Behavior
/// This IR is similar to Prettier's `conditionalGroup`. The printer measures each variant, except the [`MostExpanded`], in [`Flat`] mode
/// to find the first variant that fits and prints this variant in [`Flat`] mode. If no variant fits, then
/// the printer falls back to printing the [`MostExpanded`] variant in [`Expanded`] mode.
///
/// The definition of *fits* differs to groups in that the printer only tests if it is possible to print
/// the content up to the first non-soft line break without exceeding the configured print width.
/// This definition differs from groups as that non-soft line breaks make group expand.
///
/// [`crate::BestFitting`] acts as a "break" boundary, meaning that it is considered to fit
///
///
/// [`Flat`]: crate::format_element::PrintMode::Flat
/// [`Expanded`]: crate::format_element::PrintMode::Expanded
/// [`MostExpanded`]: crate::format_element::BestFittingVariants::most_expanded
#[macro_export]
macro_rules! best_fitting {
    ($least_expanded:expr, $($tail:expr),+ $(,)?) => {{
        // OK because the macro syntax requires at least two variants.
        $crate::BestFitting::from_arguments_unchecked($crate::format_args!($least_expanded, $($tail),+))
    }}
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::{write, FormatState, SimpleFormatOptions, VecBuffer};

    struct TestFormat;

    impl Format<SimpleFormatContext> for TestFormat {
        fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
            write!(f, [token("test")])
        }
    }

    #[test]
    fn test_single_element() {
        let mut state = FormatState::new(SimpleFormatContext::default());
        let mut buffer = VecBuffer::new(&mut state);

        write![&mut buffer, [TestFormat]].unwrap();

        assert_eq!(
            buffer.into_vec(),
            vec![FormatElement::Token { text: "test" }]
        );
    }

    #[test]
    fn test_multiple_elements() {
        let mut state = FormatState::new(SimpleFormatContext::default());
        let mut buffer = VecBuffer::new(&mut state);

        write![
            &mut buffer,
            [token("a"), space(), token("simple"), space(), TestFormat]
        ]
        .unwrap();

        assert_eq!(
            buffer.into_vec(),
            vec![
                FormatElement::Token { text: "a" },
                FormatElement::Space,
                FormatElement::Token { text: "simple" },
                FormatElement::Space,
                FormatElement::Token { text: "test" }
            ]
        );
    }

    #[test]
    fn best_fitting_variants_print_as_lists() {
        use crate::prelude::*;
        use crate::{format, format_args, Formatted};

        // The second variant below should be selected when printing at a width of 30
        let formatted_best_fitting = format!(
            SimpleFormatContext::default(),
            [
                token("aVeryLongIdentifier"),
                soft_line_break_or_space(),
                best_fitting![
                    format_args![token(
                        "Something that will not fit on a line with 30 character print width."
                    )],
                    format_args![group(&format_args![
                        token("Start"),
                        soft_line_break(),
                        group(&soft_block_indent(&format_args![
                            token("1,"),
                            soft_line_break_or_space(),
                            token("2,"),
                            soft_line_break_or_space(),
                            token("3"),
                        ])),
                        soft_line_break_or_space(),
                        soft_block_indent(&format_args![
                            token("1,"),
                            soft_line_break_or_space(),
                            token("2,"),
                            soft_line_break_or_space(),
                            group(&format_args!(
                                token("A,"),
                                soft_line_break_or_space(),
                                token("B")
                            )),
                            soft_line_break_or_space(),
                            token("3")
                        ]),
                        soft_line_break_or_space(),
                        token("End")
                    ])
                    .should_expand(true)],
                    format_args!(token("Most"), hard_line_break(), token("Expanded"))
                ]
            ]
        )
        .unwrap();

        // This matches the IR above except that the `best_fitting` was replaced with
        // the contents of its second variant.
        let formatted_normal_list = format!(
            SimpleFormatContext::default(),
            [
                token("aVeryLongIdentifier"),
                soft_line_break_or_space(),
                format_args![
                    token("Start"),
                    soft_line_break(),
                    &group(&soft_block_indent(&format_args![
                        token("1,"),
                        soft_line_break_or_space(),
                        token("2,"),
                        soft_line_break_or_space(),
                        token("3"),
                    ])),
                    soft_line_break_or_space(),
                    &soft_block_indent(&format_args![
                        token("1,"),
                        soft_line_break_or_space(),
                        token("2,"),
                        soft_line_break_or_space(),
                        group(&format_args!(
                            token("A,"),
                            soft_line_break_or_space(),
                            token("B")
                        )),
                        soft_line_break_or_space(),
                        token("3")
                    ]),
                    soft_line_break_or_space(),
                    token("End")
                ],
            ]
        )
        .unwrap();

        let best_fitting_code = Formatted::new(
            formatted_best_fitting.into_document(),
            SimpleFormatContext::new(SimpleFormatOptions {
                line_width: 30.try_into().unwrap(),
                ..SimpleFormatOptions::default()
            }),
        )
        .print()
        .expect("Document to be valid")
        .as_code()
        .to_string();

        let normal_list_code = Formatted::new(
            formatted_normal_list.into_document(),
            SimpleFormatContext::new(SimpleFormatOptions {
                line_width: 30.try_into().unwrap(),
                ..SimpleFormatOptions::default()
            }),
        )
        .print()
        .expect("Document to be valid")
        .as_code()
        .to_string();

        // The variant that "fits" will print its contents as if it were a normal list
        // outside of a BestFitting element.
        assert_eq!(best_fitting_code, normal_list_code);
    }
}
