This is a fork of the [`annotate-snippets` crate]. The principle motivation for
this fork, at the time of writing, is [issue #167]. Specifically, we wanted to
upgrade our version of `annotate-snippets`, but do so _without_ changing our
diagnostic message format.

This copy of `annotate-snippets` is basically identical to upstream, but with
an extra `Level::None` variant that permits skipping over a new non-optional
header emitted by `annotate-snippets`.

More generally, it seems plausible that we may want to tweak other aspects of
the output format in the future, so it might make sense to stick with our own
copy so that we can be masters of our own destiny.

[issue #167]: https://github.com/rust-lang/annotate-snippets-rs/issues/167
[`annotate-snippets` crate]: https://github.com/rust-lang/annotate-snippets-rs
