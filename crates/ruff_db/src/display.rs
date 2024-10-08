use std::fmt::{self, Display, Formatter};

pub trait FormatterJoinExtension<'b> {
    fn join<'a>(&'a mut self, separator: &'static str) -> Join<'a, 'b>;
}

impl<'b> FormatterJoinExtension<'b> for Formatter<'b> {
    fn join<'a>(&'a mut self, separator: &'static str) -> Join<'a, 'b> {
        Join {
            fmt: self,
            separator,
            result: fmt::Result::Ok(()),
            seen_first: false,
        }
    }
}

pub struct Join<'a, 'b> {
    fmt: &'a mut Formatter<'b>,
    separator: &'static str,
    result: fmt::Result,
    seen_first: bool,
}

impl<'a, 'b> Join<'a, 'b> {
    pub fn entry(&mut self, item: &dyn Display) -> &mut Self {
        if self.seen_first {
            self.result = self
                .result
                .and_then(|()| self.fmt.write_str(self.separator));
        } else {
            self.seen_first = true;
        }
        self.result = self.result.and_then(|()| item.fmt(self.fmt));
        self
    }

    pub fn entries<I, F>(&mut self, items: I) -> &mut Self
    where
        I: IntoIterator<Item = F>,
        F: Display,
    {
        for item in items {
            self.entry(&item);
        }
        self
    }

    pub fn finish(&mut self) -> fmt::Result {
        self.result
    }
}
