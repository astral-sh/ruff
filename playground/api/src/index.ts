/**
 * A Workers KV-based database for storing shareable code snippets in the Playground.
 */

export interface Env {
  // The Workers KV namespace to use for storing code snippets.
  PLAYGROUND: KVNamespace;
  // Whether or not we're in a development environment.
  DEV?: boolean;
}

const HEADERS = {
  "Access-Control-Allow-Origin": "*",
};

export default {
  async fetch(
    request: Request,
    env: Env,
    ctx: ExecutionContext,
  ): Promise<Response> {
    const { DEV, PLAYGROUND } = env;

    // Verify that we're either in a development environment or that the request
    // came from `https://play.ruff.rs`.
    if (!DEV) {
      const { origin } = new URL(request.url);
      if (origin !== "https://play.ruff.rs") {
        return new Response("Unauthorized", {
          status: 401,
          headers: HEADERS,
        });
      }
    }

    // URLs take the form `https://api.astral-1ad.workers.dev/<key>`. A `GET` request
    // will return the value associated with the key, while a `POST` request will
    // set the value associated with the key.
    const { pathname } = new URL(request.url);
    const key = pathname.slice(1);
    if (!key) {
      return new Response("Not Found", {
        status: 404,
        headers: HEADERS,
      });
    }

    switch (request.method) {
      case "GET": {
        const value = await PLAYGROUND.get(key);
        if (value === null) {
          return new Response("Not Found", {
            status: 404,
            headers: HEADERS,
          });
        }
        return new Response(value, {
          status: 200,
          headers: HEADERS,
        });
      }

      case "POST": {
        const value = await request.text();
        await PLAYGROUND.put(key, value);
        return new Response("OK", {
          status: 200,
          headers: HEADERS,
        });
      }

      default: {
        return new Response("Method Not Allowed", {
          status: 405,
          headers: HEADERS,
        });
      }
    }
  },
};
