# Ruff WASM

> **⚠️ WARNING: This API is experimental and may change at any time**

[**Docs**](https://docs.astral.sh/ruff/) | [**Playground**](https://play.ruff.rs/)

An extremely fast Python linter and code formatter, written in Rust.

This is a WASM version of the Ruff API which can be used to lint/format Python in a browser environment.

There are multiple versions for the different wasm-pack targets. See [here](https://rustwasm.github.io/docs/wasm-bindgen/reference/deployment.html) for more info on targets.

- [Bundler](https://www.npmjs.com/package/@astral-sh/ruff-wasm-bundler)
- [Web](https://www.npmjs.com/package/@astral-sh/ruff-wasm-web)
- [Node.js](https://www.npmjs.com/package/@astral-sh/ruff-wasm-nodejs)

## Usage

This example uses the wasm-pack web target and is known to work with Vite.

```ts
import init, { Workspace, type Diagnostic, PositionEncoding } from '@astral-sh/ruff-wasm-web';

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
}, PositionEncoding.UTF16);

// Will contain 1 diagnostic code for E702: Multiple statements on one line
const diagnostics: Diagnostic[] = workspace.check(exampleDocument);

const formatted = workspace.format(exampleDocument);
```
