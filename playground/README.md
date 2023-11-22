# playground

In-browser playground for Ruff. Available [https://play.ruff.rs/](https://play.ruff.rs/).

## Getting started

In order to build the WASM module install [`wasm-pack`](https://rustwasm.github.io/wasm-pack/).

Next, build the WASM module by running `npm run build:wasm` (release build) or `npm run dev:wasm` (debug build) from the `./playground` directory.

Finally, install TypeScript dependencies with `npm install`, and run the development server with `npm run dev`.

To run the datastore, which is based on [Workers KV](https://developers.cloudflare.com/workers/runtime-apis/kv/),
install the [Wrangler CLI](https://developers.cloudflare.com/workers/wrangler/install-and-update/),
then run `npx wrangler dev --local` from the `./playground/db` directory. Note that the datastore is
only required to generate shareable URLs for code snippets. The development datastore does not
require Cloudflare authentication or login, but in turn only persists data locally.

## Architecture

The playground is implemented as a single-page React application powered by
[Vite](https://vitejs.dev/), with the editor experience itself powered by
[Monaco](https://github.com/microsoft/monaco-editor).

The playground stores state in `localStorage`, but supports persisting code snippets to
a persistent datastore based on [Workers KV](https://developers.cloudflare.com/workers/runtime-apis/kv/)
and exposed via a [Cloudflare Worker](https://developers.cloudflare.com/workers/learning/how-workers-works/).

The playground design is originally based on [Tailwind Play](https://play.tailwindcss.com/), with
additional inspiration from the [Rome Tools Playground](https://docs.rome.tools/playground/).
