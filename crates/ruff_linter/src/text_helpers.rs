use std::borrow::Cow;

pub(crate) trait ShowNonprinting {
    fn show_nonprinting(&self) -> Cow<'_, str>;
}

macro_rules! impl_show_nonprinting {
    ($(($from:expr, $to:expr)),+) => {
        impl ShowNonprinting for str {
            fn show_nonprinting(&self) -> Cow<'_, str> {
                if self.find(&[$($from),*][..]).is_some() {
                    Cow::Owned(
                        self.$(replace($from, $to)).*
                    )
                } else {
                    Cow::Borrowed(self)
                }
            }
        }
    };
}

impl_show_nonprinting!(('\x07', "␇"), ('\x08', "␈"), ('\x1b', "␛"), ('\x7f', "␡"));
