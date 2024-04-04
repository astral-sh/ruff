use anstream::stream::RawStream;
use anstream::{AutoStream, ColorChoice};

pub fn none<S: RawStream>(stream: S) -> AutoStream<S> {
    AutoStream::new(stream, ColorChoice::Never)
}

pub fn auto<S: RawStream>(stream: S) -> AutoStream<S> {
    let choice = choice(&stream);
    AutoStream::new(stream, choice)
}

pub fn choice<S: RawStream>(stream: &S) -> ColorChoice {
    AutoStream::choice(stream)
}

pub fn enabled<S: RawStream>(stream: &S) -> bool {
    choice(stream) != ColorChoice::Never
}
