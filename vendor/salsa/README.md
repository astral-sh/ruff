# salsa

[![Test](https://github.com/salsa-rs/salsa/workflows/Test/badge.svg)](https://github.com/salsa-rs/salsa/actions?query=workflow%3ATest)
[![Book](https://github.com/salsa-rs/salsa/workflows/Book/badge.svg)](https://github.com/salsa-rs/salsa/actions?query=workflow%3ABook)
[![Released API docs](https://docs.rs/salsa/badge.svg)](https://docs.rs/salsa)
[![Crates.io](https://img.shields.io/crates/v/salsa.svg)](https://crates.io/crates/salsa)

*A generic framework for on-demand, incrementalized computation.*

<img alt="Salsa Logo" src="https://raw.githubusercontent.com/salsa-rs/logo/main/FerrisSalsa4-01.svg" width="200" />

## Obligatory warning

Very much a WORK IN PROGRESS at this point.

## Credits

This system is heavily inspired by [adapton](http://adapton.org/), [glimmer](https://github.com/glimmerjs/glimmer-vm), and rustc's query
system. So credit goes to Eduard-Mihai Burtescu, Matthew Hammer,
Yehuda Katz, and Michael Woerister.

## Key idea

The key idea of `salsa` is that you define your program as a set of
**queries**. Every query is used like function `K -> V` that maps from
some key of type `K` to a value of type `V`. Queries come in two basic
varieties:

- **Inputs**: the base inputs to your system. You can change these
  whenever you like.
- **Functions**: pure functions (no side effects) that transform your
  inputs into other values. The results of queries are memoized to
  avoid recomputing them a lot. When you make changes to the inputs,
  we'll figure out (fairly intelligently) when we can re-use these
  memoized values and when we have to recompute them.

## Want to learn more?

To learn more about Salsa, try one of the following:

- read the [heavily commented examples](https://github.com/salsa-rs/salsa/tree/master/examples);
- check out the [Salsa book](https://salsa-rs.github.io/salsa);
    - [中文版](https://rust-chinese-translation.github.io/salsa-book)
- watch one of our [videos](https://salsa-rs.github.io/salsa/videos.html).

## Getting in touch

The bulk of the discussion happens in the [issues](https://github.com/salsa-rs/salsa/issues)
and [pull requests](https://github.com/salsa-rs/salsa/pulls),
but we have a [zulip chat](https://salsa.zulipchat.com/) as well.

## Contributing

To create a release and publish to crates.io, update the `version` field in Cargo.toml.
After pushed, GitHub Actions will publish the crates to crates.io automatically.
