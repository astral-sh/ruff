"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const { classify, githubRequest, passed, probe } = require("./index.cjs");

const uploadUrl = "https://upload.example.invalid/object?secret=signed-url";
const environment = {
  GITHUB_REPOSITORY: "astral-sh/ruff",
  PROBE_HEAD_REPOSITORY: "astral-sh/ruff",
  GITHUB_EVENT_NAME: "pull_request",
  GITHUB_HEAD_REF: "zsol/depot-cache-mode-probe",
  GITHUB_BASE_REF: "main",
  GITHUB_ACTOR: "zsol",
  GITHUB_RUN_ID: "123",
  GITHUB_RUN_ATTEMPT: "1",
  INPUT_BACKEND: "depot",
  INPUT_MODE: "write",
  ACTIONS_CACHE_MODE: "write",
  ACTIONS_CACHE_SERVICE_V2: "True",
  ACTIONS_RESULTS_URL: "https://results.example.invalid/base/",
  ACTIONS_RUNTIME_TOKEN: "runtime-test-token",
  DEPOT_CACHE_TOKEN: "depot-test-token",
};

test("uses the advertised GitHub cache protocol", () => {
  const v2 = githubRequest(environment, "unique-v2-key");
  assert.equal(v2.surface, "github-v2");
  assert.equal(
    v2.url.href,
    "https://results.example.invalid/twirp/github.actions.results.api.v1.CacheService/CreateCacheEntry",
  );
  assert.deepEqual(Object.keys(v2.body), ["key", "version"]);
  assert.match(v2.body.version, /^[a-f0-9]{64}$/);

  const v1 = githubRequest(
    {
      ...environment,
      ACTIONS_CACHE_SERVICE_V2: "",
      ACTIONS_CACHE_URL: "https://cache.example.invalid/namespace/",
    },
    "unique-v1-key",
  );
  assert.equal(v1.surface, "github-v1");
  assert.equal(
    v1.url.href,
    "https://cache.example.invalid/namespace/_apis/artifactcache/caches",
  );
  assert.equal(v1.body.cacheSize, 1);
  assert.equal(v1.headers.Accept, "application/json;api-version=6.0-preview.1");
});

test("requires a complete successful reservation response", () => {
  assert.equal(classify("github-v1", 201, { cacheId: 1 }).result, "write-grant-issued");
  assert.equal(classify("github-v1", 201, { cacheId: 0 }).result, "inconclusive");
  assert.equal(
    classify("github-v2", 200, { ok: true, signed_upload_url: uploadUrl }).result,
    "write-grant-issued",
  );
  assert.equal(classify("github-v2", 200, { ok: true }).result, "inconclusive");
  assert.equal(
    classify("depot-native", 200, { entryId: "entry", uploadPartUrls: [uploadUrl] }).result,
    "write-grant-issued",
  );
  assert.equal(
    classify("depot-native", 200, { entryId: "entry", uploadPartUrls: [null] }).result,
    "inconclusive",
  );
});

test("does not mistake protocol or authentication failures for a policy denial", () => {
  for (const status of [400, 404, 409, 429, 500]) {
    assert.equal(classify("github-v2", status, {}).result, "inconclusive");
  }
  for (const surface of ["github-v1", "github-v2", "depot-native"]) {
    assert.equal(classify(surface, 403, null).result, "inconclusive");
  }
  assert.equal(classify("depot-native", 401, {}).result, "unauthenticated");
  assert.equal(
    classify("github-v2", 200, { ok: false, message: "cache write denied: read-only" }).result,
    "write-denied",
  );
  assert.equal(
    classify("depot-native", 403, { code: "permission_denied" }).result,
    "write-denied",
  );
});

test("uses separate credentials and stops after the two reservations", async () => {
  const calls = [];
  const summary = await probe(environment, async (url, options) => {
    calls.push({ url: url.href, ...options });
    const body = url.hostname === "cache.depot.dev"
      ? { entryId: "entry", uploadPartUrls: [uploadUrl] }
      : { ok: true, signed_upload_url: uploadUrl };
    return { status: 200, json: async () => body };
  });

  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call.method === "POST" && call.redirect === "error"));
  assert.equal(calls[0].headers.Authorization, "Bearer runtime-test-token");
  assert.equal(calls[1].headers.Authorization, "Bearer depot-test-token");
  assert.equal(calls[1].url, "https://cache.depot.dev/depot.cache.v1.CacheService/CreateEntry");
  assert.equal(calls[1].headers["Connect-Protocol-Version"], "1");
  const nativeBody = JSON.parse(calls[1].body);
  assert.equal(nativeBody.entryType, "generic");
  assert.equal(nativeBody.failIfUploadInProgress, true);
  assert.match(nativeBody.key, /^[a-f0-9]{64}$/);
  assert.ok(passed(summary));
  assert.equal(summary.results[0].endpointDomain, "other");
  assert.equal(summary.results[1].endpointDomain, "depot.dev");
  assert.ok(summary.results.every((result) => result.credentialIssued));
  for (const secret of [uploadUrl, "entry", "runtime-test-token", "depot-test-token"]) {
    assert.ok(!JSON.stringify(summary).includes(secret));
  }
});

test("fails read mode when the backend grants write authority", async () => {
  const summary = await probe(
    { ...environment, INPUT_MODE: "read", ACTIONS_CACHE_MODE: "read" },
    async (url) => ({
      status: 200,
      json: async () => url.hostname === "cache.depot.dev"
        ? { entryId: "entry", uploadPartUrls: [uploadUrl] }
        : { ok: true, signed_upload_url: uploadUrl },
    }),
  );
  assert.equal(summary.advertisedMode, "read");
  assert.ok(!passed(summary));
});

test("accepts a runner-local HTTP cache proxy without allowing remote HTTP", async () => {
  for (const host of ["127.0.0.1:40000", "[::1]:40000", "localhost:40000"]) {
    let requests = 0;
    const summary = await probe(
      {
        ...environment,
        INPUT_BACKEND: "github",
        ACTIONS_RESULTS_URL: `http://${host}/`,
        DEPOT_CACHE_TOKEN: "",
      },
      async (url) => {
        requests++;
        assert.equal(url.host, host);
        return { status: 200, json: async () => ({ ok: true, signed_upload_url: uploadUrl }) };
      },
    );
    assert.equal(requests, 1);
    assert.equal(summary.results[0].endpointDomain, "loopback");
    assert.equal(summary.results[0].endpointScheme, "http");
    assert.ok(passed(summary));
  }

  for (const host of ["remote.example.invalid", "127.0.0.1.example.invalid", "10.0.0.1"]) {
    const summary = await probe(
      {
        ...environment,
        INPUT_BACKEND: "github",
        ACTIONS_RESULTS_URL: `http://${host}/`,
        DEPOT_CACHE_TOKEN: "",
      },
      async () => assert.fail("non-loopback HTTP request"),
    );
    assert.equal(summary.results[0].result, "invalid-endpoint");
    assert.equal(summary.results[0].endpointScheme, "http");
    assert.ok(!passed(summary));
  }
});

test("redacts transport failures and refuses an unintended repository", async () => {
  const summary = await probe(environment, async () => {
    throw new Error(`request to ${uploadUrl} with ${environment.DEPOT_CACHE_TOKEN}`);
  });
  assert.ok(summary.results.every((result) => result.result === "request-failed"));
  assert.ok(!JSON.stringify(summary).includes("signed-url"));
  assert.ok(!JSON.stringify(summary).includes("depot-test-token"));
  await assert.rejects(
    probe({ ...environment, PROBE_HEAD_REPOSITORY: "someone/ruff" }, async () => {
      assert.fail("out-of-scope request");
    }),
    /outside its configured scope/,
  );
});
