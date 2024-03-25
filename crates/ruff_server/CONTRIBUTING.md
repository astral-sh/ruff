## Contributing to the Ruff Language Server

This is a mostly free-form guide with resources to help you get started with contributing to `ruff server`.

### Project Architecture

`ruff_server` uses a [lock-free data model](https://github.com/astral-sh/ruff/blob/main/crates/ruff_server/src/session.rs) to represent its state. The server runs in a [continuous event loop](https://github.com/astral-sh/ruff/blob/main/crates/ruff_server/src/server.rs) by listening to incoming messages
over `stdin` and dispatches [tasks](https://github.com/astral-sh/ruff/blob/main/crates/ruff_server/src/server/schedule/task.rs) based on the type of message. A 'task' can either be 'local' or 'background' - the former kind has
exclusive mutable access to the state and execute immediately, blocking the event loop until their completion. The latter kind, background
tasks, run immediately on a thread pool with an immutable snapshot of the state, and do _not_ block the event loop unless the thread pool
queue is full, in which case the server will block on available queue space.

[Snapshots of the server state](https://github.com/astral-sh/ruff/blob/main/crates/ruff_server/src/session.rs) use atomic reference-counted pointers (`Arc`) to prevent unnecessary cloning of large text files. If the contents
of the text file need to be updated, the state will create a new `Arc` to store it. This allows a local task to run at the same time as multiple background tasks
without changing the state the background tasks are working with. This only applies to background tasks started _before_ the local task though, as a local task blocks
the handling of further messages (and therefore, dispatching future tasks) until its completion.

`ruff_server` uses the `lsp-server` and `lsp-types` crates in favor of a more involved framework like `tower-lsp` because of the flexibility that the former gives us
in our implementation. A goal for this project was to take an architectural approach similar to `rust-analyzer`, with locally running tasks that access the state exclusively,
along with background tasks that reference a snapshot of the state. `tower-lsp` would have given us less control over execution order, which may have required us to use locks
or other thread synchronization methods to ensure data integrity. In fact, the `tower-lsp` scheduler has [existing issues](https://github.com/ebkalderon/tower-lsp/issues/284) around
data races and out-of-order handler execution. Our approach avoids this issue by dis-allowing `async` in tasks and using a scheduler focused on data mutability and access.

### Testing

Most editors with LSP support (VS Code is a notable exception) will let you provide a server command for that language server. Ruff's server command is `<ruff binary> server --preview`, which will begin running a language server that listens to messages on `stdin` and responds on `stdout`.

Unless you've already installed your local `ruff` build in your `PATH` (in which case you can just use `ruff server --preview` as the server command) the server command should be the path to your locally-built ruff executable (usually `<path to your ruff source>/target/debug/ruff`) along with the arguments `server` and `--preview`. Make sure to (re)build the server with `cargo build -p ruff`!

#### Testing In VS Code

At the moment, the [pre-release version](https://github.com/astral-sh/ruff-vscode/tree/pre-release) of Ruff's new VS Code extension only has the option to use the bundled Ruff binary. Configuration to use a custom Ruff executable path will be ready soon.
