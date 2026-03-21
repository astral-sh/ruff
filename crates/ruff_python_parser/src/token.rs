use ruff_python_ast::{Int, IpyEscapeKind, name::Name};

#[derive(Clone, Debug, Default)]
pub(crate) enum TokenValue {
    #[default]
    None,
    /// Token value for a name, commonly known as an identifier.
    ///
    /// Unicode names are NFKC-normalized by the lexer,
    /// matching [the behaviour of Python's lexer](https://docs.python.org/3/reference/lexical_analysis.html#identifiers)
    Name(Name),
    /// Token value for an integer.
    Int(Int),
    /// Token value for a floating point number.
    Float(f64),
    /// Token value for a complex number.
    Complex {
        /// The real part of the complex number.
        real: f64,
        /// The imaginary part of the complex number.
        imag: f64,
    },
    /// Token value for a string.
    String(Box<str>),
    /// Token value that includes the portion of text inside the f-string that's not
    /// part of the expression part and isn't an opening or closing brace.
    InterpolatedStringMiddle(Box<str>),
    /// Token value for IPython escape commands. These are recognized by the lexer
    /// only when the mode is [`Mode::Ipython`].
    IpyEscapeCommand {
        /// The magic command value.
        value: Box<str>,
        /// The kind of magic command.
        kind: IpyEscapeKind,
    },
}
