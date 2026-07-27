# The Ruff Language Server

`ruff server` is a language server that powers Ruff's editor integrations.

The job of the language server is to listen for requests from the client (in this case, the code editor of your choice)
and call into Ruff's linter and formatter crates to construct real-time diagnostics or formatted code, which is then
sent back to the client. It also tracks configuration files in your editor's workspace, and will refresh its in-memory
configuration whenever those files are modified.

Refer to the [documentation](https://docs.astral.sh/ruff/editors/) for more information on
how to set up the language server with your editor and configure it to your liking.

## Contributing

Contributions are welcome and highly appreciated. To get started, check out the
[**contributing guidelines**](https://docs.astral.sh/ruff/contributing/).

You can also join us on [**Discord**](https://discord.com/invite/astral-sh).

## Versioning

<!-- BEGIN GENERATED CRATE VERSIONING -->

This crate is an internal component of [Ruff](https://crates.io/crates/ruff). The Rust API exposed
here is unstable and will have frequent breaking changes.

This version (0.0.6) is a component of [Ruff 0.16.0](https://crates.io/crates/ruff/0.16.0). The
source can be found [here](https://github.com/astral-sh/ruff/blob/0.16.0/crates/ruff_server).

See Ruff's [crate versioning policy](https://docs.astral.sh/ruff/versioning/#crate-versioning) for
details on versioning.

<!-- END GENERATED CRATE VERSIONING -->
