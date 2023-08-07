# db

Key-value store based on [Workers KV](https://developers.cloudflare.com/workers/runtime-apis/kv/).

Used to persist code snippets in the playground and generate shareable URLs.

## Getting started

To run locally, login via `wrangler`, and run `wrangler dev`. The worker will run on `localhost:8787`.
When run in development node, playground will automatically send requests to the worker development
server.

To deploy, run `wrangler publish`.
