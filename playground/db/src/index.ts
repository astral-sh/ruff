/**
 * A Workers KV-based database for storing shareable code snippets in the Playground.
 */

export interface Env {
  PLAYGROUND: KVNamespace;
}

export default {
  async fetch(
    request: Request,
    env: Env,
    ctx: ExecutionContext,
  ): Promise<Response> {
    const headers = {
      "Access-Control-Allow-Origin": "*",
    };

    switch (request.method) {
      case "GET": {
        const { PLAYGROUND } = env;
        const { pathname } = new URL(request.url);
        const key = pathname.slice(1);
        const value = await PLAYGROUND.get(key);
        if (value === null) {
          return new Response("Not Found", {
            status: 404,
            headers,
          });
        }
        return new Response(value, {
          status: 200,
          headers,
        });
      }

      case "POST": {
        const { PLAYGROUND } = env;
        const { pathname } = new URL(request.url);
        const key = pathname.slice(1);
        const value = await request.text();
        await PLAYGROUND.put(key, value);
        return new Response("OK", {
          status: 200,
          headers,
        });
      }

      case "DELETE": {
        const { PLAYGROUND } = env;
        const { pathname } = new URL(request.url);
        const key = pathname.slice(1);
        await PLAYGROUND.delete(key);
        return new Response("OK", {
          status: 200,
          headers,
        });
      }

      default: {
        return new Response("Method Not Allowed", {
          status: 405,
          headers,
        });
      }
    }
  },
};
