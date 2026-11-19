
import assert from "node:assert/strict";
import { Readable } from "node:stream";
import { test } from "node:test";
import { syncEvents } from "../scripts/accounting-archive-sync.mjs";

const config = {
  origin: "https://accounting.example.invalid",
  token: "synthetic-token",
  company: "company-1",
  source: "archive-2024",
};
const input = (...sequences) => Readable.from(sequences.map(sequence =>
  JSON.stringify({ sequence, change: { kind: "revoke" } }) + "\n"
));

test("archive sync resumes after a confirmed checkpoint and verifies every receipt", async () => {
  const received = [];
  const last = await syncEvents({
    ...config,
    input: input(1, 2, 3),
    after: 1,
    send: async (url, request) => {
      assert.equal(url.pathname, "/api/accounting-extension/archive/company-1/archive-2024/events");
      assert.equal(request.redirect, "error");
      assert.equal(request.headers.authorization, "Bearer synthetic-token");
      const { sequence } = JSON.parse(request.body);
      received.push(sequence);
      return Response.json({ sequence });
    },
  });
  assert.deepEqual(received, [2, 3]);
  assert.equal(last, 3);
});

test("archive sync blocks gaps and ambiguous receipts", async () => {
  await assert.rejects(syncEvents({
    ...config, input: input(2),
    send: async () => assert.fail("gap reached transport"),
  }), /sequence gap/);
  await assert.rejects(syncEvents({
    ...config, input: input(1),
    send: async () => Response.json({ sequence: 2 }),
  }), /did not match/);
  await assert.rejects(syncEvents({
    ...config, input: input(1),
    send: async () => { throw new Error("sensitive upstream detail"); },
  }), /No acknowledgment for archive event 1/);
});
