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
