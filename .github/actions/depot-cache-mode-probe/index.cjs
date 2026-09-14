"use strict";

const { createHash, randomUUID } = require("node:crypto");
const { appendFileSync } = require("node:fs");

const REPOSITORY = "astral-sh/ruff";
const BRANCH = "zsol/depot-cache-mode-probe";
const MODES = new Set(["read", "write", "write-only", "none"]);
const ERROR_CODES = new Set([
  "permission_denied",
  "unauthenticated",
  "invalid_argument",
  "not_found",
  "already_exists",
  "resource_exhausted",
  "unavailable",
  "unimplemented",
  "internal",
  "deadline_exceeded",
]);
const VERSION = createHash("sha256")
  .update("ruff-depot-cache-mode-probe-v1")
  .digest("hex");

function isUploadUrl(value) {
  if (typeof value !== "string") {
    return false;
  }
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}

function classify(surface, status, body) {
  const code = ERROR_CODES.has(body?.code) ? body.code : null;
  const policyDenied = [body?.message, body?.msg, body?.error?.message].some(
    (message) =>
      typeof message === "string" && message.startsWith("cache write denied:"),
  );
  const granted =
    status >= 200 &&
    status < 300 &&
    (surface === "github-v1"
      ? Number.isSafeInteger(body?.cacheId) && body.cacheId > 0
      : surface === "github-v2"
        ? body?.ok === true &&
          isUploadUrl(body.signed_upload_url ?? body.signedUploadUrl)
        : typeof (body?.entryId ?? body?.entry_id) === "string" &&
          (body.entryId ?? body.entry_id).length > 0 &&
          Array.isArray(body.uploadPartUrls ?? body.upload_part_urls) &&
          (body.uploadPartUrls ?? body.upload_part_urls).length > 0 &&
          (body.uploadPartUrls ?? body.upload_part_urls).every(isUploadUrl));

  let result = "inconclusive";
  if (granted) {
    result = "write-grant-issued";
  } else if (
    (status === 403 && code === "permission_denied") ||
    (surface !== "depot-native" &&
      policyDenied &&
      (status === 403 || (status >= 200 && status < 300)))
  ) {
    result = "write-denied";
  } else if (status === 401 || code === "unauthenticated") {
    result = "unauthenticated";
  }
  return { surface, result, httpStatus: status, errorCode: code, policyDenied };
}

function githubRequest(env, key) {
  if (env.ACTIONS_CACHE_SERVICE_V2) {
    return {
      surface: "github-v2",
      url: new URL(
        "/twirp/github.actions.results.api.v1.CacheService/CreateCacheEntry",
        env.ACTIONS_RESULTS_URL,
      ),
      body: { key, version: VERSION },
    };
  }

  const base = env.ACTIONS_CACHE_URL || env.ACTIONS_RESULTS_URL;
  return {
    surface: "github-v1",
    url: new URL(
      "_apis/artifactcache/caches",
      base?.endsWith("/") ? base : `${base}/`,
    ),
    body: { key, version: VERSION, cacheSize: 1 },
    headers: { Accept: "application/json;api-version=6.0-preview.1" },
  };
}

function endpointDomain(url) {
  if (url.hostname === "depot.dev" || url.hostname.endsWith(".depot.dev")) {
    return "depot.dev";
  }
  if (url.hostname.endsWith(".actions.githubusercontent.com")) {
    return "actions.githubusercontent.com";
  }
  return "other";
}

async function reserve(request, token, fetcher) {
  const { url } = request;
  const details = {
    surface: request.surface,
    credentialIssued: Boolean(token),
    endpointDomain: endpointDomain(url),
  };
  if (!token) {
    return { ...details, result: "credential-not-issued" };
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    return { ...details, result: "invalid-endpoint" };
  }

  try {
    const response = await fetcher(url, {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(15_000),
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
        ...request.headers,
      },
      body: JSON.stringify(request.body),
    });
    const body = await response.json().catch(() => null);
    return { ...details, ...classify(request.surface, response.status, body) };
  } catch {
    // Raw errors and response bodies can contain credentials or signed URLs.
    return { ...details, result: "request-failed" };
  }
}

async function probe(env = process.env, fetcher = fetch) {
  const backend = env.INPUT_BACKEND;
  const mode = env.INPUT_MODE;
  if (
    env.GITHUB_REPOSITORY !== REPOSITORY ||
    env.PROBE_HEAD_REPOSITORY !== REPOSITORY ||
    env.GITHUB_EVENT_NAME !== "pull_request" ||
    env.GITHUB_HEAD_REF !== BRANCH ||
    env.GITHUB_BASE_REF !== "main" ||
    env.GITHUB_ACTOR !== "zsol" ||
    !["github", "depot"].includes(backend) ||
    !["read", "write"].includes(mode)
  ) {
    throw new Error("The cache probe is outside its configured scope.");
  }

  const newKey = (surface) =>
    `ruff-cache-mode-probe-${env.GITHUB_RUN_ID}-${env.GITHUB_RUN_ATTEMPT}-${surface}-${randomUUID()}`;
  const advertised = env.ACTIONS_CACHE_MODE;
  const summary = {
    backend,
    requestedMode: mode,
    advertisedMode: MODES.has(advertised)
      ? advertised
      : advertised
        ? "unrecognized"
        : "not-issued",
    results: [],
  };

  // Use the same reservation protocols as actions/toolkit, but bypass its
  // cooperative ACTIONS_CACHE_MODE check. Do not upload or finalize anything.
  try {
    summary.results.push(
      await reserve(
        githubRequest(env, newKey("github")),
        env.ACTIONS_RUNTIME_TOKEN,
        fetcher,
      ),
    );
  } catch {
    summary.results.push({
      surface: env.ACTIONS_CACHE_SERVICE_V2 ? "github-v2" : "github-v1",
      result: "invalid-endpoint",
    });
  }

  if (backend === "depot") {
    const base = new URL(env.DEPOT_CACHE_HOST || "https://cache.depot.dev");
    if (base.origin !== "https://cache.depot.dev") {
      summary.results.push({
        surface: "depot-native",
        result: "unexpected-endpoint",
      });
    } else {
      // Depot's CLI uses CreateEntry before its signed-URL PUT and FinalizeEntry.
      // A fresh digest and an unfinished reservation cannot replace a live cache.
      const key = createHash("sha256").update(newKey("depot")).digest("hex");
      summary.results.push(
        await reserve(
          {
            surface: "depot-native",
            url: new URL("/depot.cache.v1.CacheService/CreateEntry", base),
            body: {
              entryType: "generic",
              key,
              failIfUploadInProgress: true,
            },
            headers: { "Connect-Protocol-Version": "1" },
          },
          env.DEPOT_CACHE_TOKEN,
          fetcher,
        ),
      );
    }
  } else if (env.DEPOT_CACHE_TOKEN) {
    summary.results.push({
      surface: "depot-native",
      result: "unexpected-credential",
    });
  }

  return summary;
}

function passed(summary) {
  return (
    summary.results.length > 0 &&
    summary.results.every(({ result }) =>
      summary.requestedMode === "write"
        ? result === "write-grant-issued"
        : result === "write-denied" || result === "credential-not-issued",
    )
  );
}

async function main() {
  const summary = await probe();
  console.log(`cache-mode-probe: ${JSON.stringify(summary)}`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(
      process.env.GITHUB_STEP_SUMMARY,
      `### ${summary.backend}: ${summary.requestedMode}\n\n` +
        `Advertised mode: \`${summary.advertisedMode}\`.\n\n` +
        "| Credential surface | Endpoint domain | Result | HTTP status | Error code |\n" +
        "| --- | --- | --- | --- | --- |\n" +
        summary.results
          .map(
            (result) =>
              `| ${result.surface} | ${result.endpointDomain ?? "—"} | ${result.result} | ${result.httpStatus ?? "—"} | ${result.errorCode ?? "—"} |`,
          )
          .join("\n") +
        "\n\nNo cache object was uploaded or finalized. Interpret read-mode denials only alongside the matching write control.\n",
    );
  }
  if (!passed(summary)) {
    process.exitCode = 1;
  }
}

module.exports = { classify, githubRequest, passed, probe };

if (require.main === module) {
  main().catch(() => {
    console.error("The cache probe could not produce a sanitized result.");
    process.exitCode = 1;
  });
}
