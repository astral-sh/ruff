# playground

In-browser playground for Ruff. Available [https://play.ruff.rs/](https://play.ruff.rs/).

## Getting started

First, build the WASM module by running `npm run build:wasm` from the `./playground` directory.

Then, install TypeScript dependencies with `npm install`, and run the development server with
`npm run dev`.

To run the datastore, which is based on [Workers KV](https://developers.cloudflare.com/workers/runtime-apis/kv/),
install the [Wrangler CLI](https://developers.cloudflare.com/workers/wrangler/install-and-update/),
then run `npx wrangler dev --local` from the `./playground/db` directory. Note that the datastore is
only required to generate shareable URLs for code snippets. The development datastore does not
require Cloudflare authentication or login, but in turn only persists data locally.

## Implementation

Design based on [Tailwind Play](https://play.tailwindcss.com/).
