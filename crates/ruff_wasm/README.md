# Ruff API

> [!WARNING]
> This API is experimental and may change at any time

[**Docs**](https://docs.astral.sh/ruff/) | [**Playground**](https://play.ruff.rs/)

An extremely fast Python linter and code formatter, written in Rust.

This is a WASM version of the Ruff API which can be used to lint/format Python in a browser environment.

## Usage

```ts
import init, { Workspace, type Diagnostic } from '@astral-sh/ruff-api';

const exampleDocument = `print('hello'); print("world")`

await init(); // Initializes WASM module

// These are default settings just to illustrate configuring Ruff
// Settings info: https://docs.astral.sh/ruff/settings
const workspace = new Workspace({
  'line-length': 88,
  'indent-width': 4,
  format: {
    'indent-style': 'space',
    'quote-style': 'double',
  },
  lint: {
    select: [
      'E4',
      'E7',
      'E9',
      'F'
    ],
  },
});

// Will contain 1 diagnostic code for E702: Multiple statements on one line
const diagnostics: Diagnostic[] = workspace.check(exampleDocument);

const formatted = workspace.format(exampleDocument);
```
