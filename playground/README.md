# playground

In-browser playground for Ruff. Available [https://ruff.pages.dev/](https://ruff.pages.dev/).

## Getting started

- To build the WASM module, run `wasm-pack build --target web --out-dir playground/src/pkg` from the
  root directory.
- Install TypeScript dependencies with: `npm install`.
- Start the development server with: `npm run dev`.

## Implementation

Design based on [Tailwind Play](https://play.tailwindcss.com/). Themed with [`ayu`](https://github.com/dempfi/ayu).
