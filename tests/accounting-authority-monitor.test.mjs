
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";
import { observeSources } from "../scripts/accounting-authority-monitor.mjs";

const entry = {
  id: "official-example", jurisdiction: "CA", url: "https://example.invalid/official",
  previous_sha256: createHash("sha256").update("original").digest("hex"),
};
test("observations distinguish changed authority and unavailable sources", async () => {
  const fetchResponse = text => async (url, options) => {
    assert.equal(url.href, entry.url);
    assert.equal(options.redirect, "error");
    return new Response(text);
  };
  const [same] = await observeSources([entry], { send: fetchResponse("original") });
  assert.equal(same.status, "unchanged");
  const [changed] = await observeSources([entry], { send: fetchResponse("amended") });
  assert.equal(changed.status, "review_required");
  assert.notEqual(changed.source_sha256, entry.previous_sha256);
  const [offline] = await observeSources([entry], { send: async () => new Response(null, { status: 500 }) });
  assert.equal(offline.status, "unavailable");
  assert.equal(offline.source_sha256, undefined);
});

test("source registry rejects duplicate IDs and non-HTTPS URLs before fetching", async () => {
  const send = async () => assert.fail("invalid registry reached transport");
  await assert.rejects(observeSources([entry, entry], { send }), /registry entry/);
  await assert.rejects(observeSources([{ ...entry, url: "http://example.invalid" }], { send }), /registry entry/);
});
