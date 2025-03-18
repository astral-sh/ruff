# playground

In-browser playground for Ruff. Available [https://play.ruff.rs/](https://play.ruff.rs/).

## Getting started

Install the NPM dependencies with `npm install`, and run, and run the development server with
`npm start --workspace ruff-playground` or `npm start --workspace knot-playground`.
You may need to restart the server after making changes to Ruff or Red Knot to re-build the WASM
module.

To run the datastore, which is based
on [Workers KV](https://developers.cloudflare.com/workers/runtime-apis/kv/),
install the [Wrangler CLI](https://developers.cloudflare.com/workers/wrangler/install-and-update/),
then run `npx wrangler dev --local` from the `./playground/api` directory. Note that the datastore
is
only required to generate shareable URLs for code snippets. The development datastore does not
require Cloudflare authentication or login, but in turn only persists data locally.

## Architecture

The playground is implemented as a single-page React application powered by
[Vite](https://vitejs.dev/), with the editor experience itself powered by
[Monaco](https://github.com/microsoft/monaco-editor).

The playground stores state in `localStorage`, but supports persisting code snippets to
a persistent datastore based
on [Workers KV](https://developers.cloudflare.com/workers/runtime-apis/kv/)
and exposed via
a [Cloudflare Worker](https://developers.cloudflare.com/workers/learning/how-workers-works/).

The playground design is originally based on [Tailwind Play](https://play.tailwindcss.com/), with
additional inspiration from the [Biome Playground](https://biomejs.dev/playground/).

## Known issues

### Stack overflows

If you see stack overflows in the playground, build the WASM module in release mode:
`npm run --workspace knot-playground build:wasm`.
