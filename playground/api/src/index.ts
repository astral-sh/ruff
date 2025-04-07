/**
 * A Workers KV-based database for storing shareable code snippets in the Playground.
 */

export interface Env {
  // The Workers KV namespace to use for storing code snippets.
  PLAYGROUND: KVNamespace;
  // Whether or not we're in a development environment.
  DEV?: boolean;
}

// See: https://developers.cloudflare.com/workers/examples/security-headers/
const DEFAULT_HEADERS = {
  "Permissions-Policy": "interest-cohort=()",
  "X-XSS-Protection": "0",
  "X-Frame-Options": "DENY",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "Cross-Origin-Embedder-Policy": 'require-corp; report-to="default";',
  "Cross-Origin-Opener-Policy": 'same-site; report-to="default";',
  "Cross-Origin-Resource-Policy": "same-site",
};

const DEVELOPMENT_HEADERS = {
  ...DEFAULT_HEADERS,
  "Access-Control-Allow-Origin": "*",
};

const PRODUCTION_HEADERS = {
  ...DEFAULT_HEADERS,
  "Access-Control-Allow-Origin": "https://play.ruff.rs",
};

export default {
  async fetch(
    request: Request,
    env: Env,
    ctx: ExecutionContext,
  ): Promise<Response> {
    const { DEV, PLAYGROUND } = env;

    const headers = DEV ? DEVELOPMENT_HEADERS : PRODUCTION_HEADERS;
    if (!DEV && request.headers.get("origin") === "https://playknot.ruff.rs") {
      headers["Access-Control-Allow-Origin"] = "https://playknot.ruff.rs";
    }

    switch (request.method) {
      case "GET": {
        // Ex) `https://api.astral-1ad.workers.dev/<key>`
        // Given an ID, return the corresponding playground.
        const { pathname } = new URL(request.url);
        const key = pathname.slice(1);
        if (!key) {
          return new Response("Not Found", {
            status: 404,
            headers,
          });
        }

        const playground = await PLAYGROUND.get(key);
        if (playground == null) {
          return new Response("Not Found", {
            status: 404,
            headers,
          });
        }

        return new Response(playground, {
          status: 200,
          headers,
        });
      }

      // Ex) `https://api.astral-1ad.workers.dev`
      // Given a playground, save it and return its ID.
      case "POST": {
        const id = crypto.randomUUID();
        const playground = await request.text();
        await PLAYGROUND.put(id, playground);
        return new Response(id, {
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
