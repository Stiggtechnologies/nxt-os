
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Readable } from "node:stream";
import { test } from "node:test";
import { buildEvents, embed } from "../scripts/accounting-sandbox-archive.mjs";
import { syncEvents } from "../scripts/accounting-archive-sync.mjs";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "nxt-sandbox-"));
  const outside = await mkdtemp(join(tmpdir(), "nxt-sandbox-outside-"));
  t.after(async () => {
    await rm(root, { recursive: true, force: true });
    await rm(outside, { recursive: true, force: true });
  });
  const text = "Synthetic rent accrual CAD 1.25";
  await writeFile(join(root, "rent.txt"), text);
  await writeFile(join(outside, "outside.txt"), text);
  const manifest = {
    classification: "synthetic_sandbox_only", after: 0,
    permissions_observed_at: "2026-11-19T00:00:00Z",
    documents: [{ id: "doc1", version: "v1", file: "rent.txt",
      sha256: createHash("sha256").update(text).digest("hex"),
      from: "2024-01-01", through: "2024-12-31", user_ids: ["usr_test"] }],
  };
  return { root, outside, text, manifest };
}

test("synthetic archive emits deterministic embeddings and ordered feeder events", async t => {
  const { root, text, manifest } = await fixture(t);
  const events = await buildEvents(root, manifest);
  assert.deepEqual(events.map(event => event.sequence), [1, 2]);
  assert.deepEqual(events[0].change.document.chunks[0].embedding, embed(text));
  assert.equal(events[1].change.observed_at, "2026-11-19T00:00:00.000Z");
  const posted = [];
  assert.equal(await syncEvents({
    origin: "https://sandbox.example.invalid", token: "synthetic", company: "test",
    source: "source-test", input: Readable.from(events.map(event => JSON.stringify(event) + "\n")),
    send: async (_, request) => {
      const event = JSON.parse(request.body);
      posted.push(event);
      return Response.json({ sequence: event.sequence });
    },
  }), 2);
  assert.deepEqual(posted, events);
});

test("archive job rejects real classifications, digest drift and outside paths", async t => {
  const { root, outside, manifest } = await fixture(t);
  await assert.rejects(buildEvents(root, { ...manifest, classification: "firm_files" }), /synthetic archive manifest/);
  await writeFile(join(root, "rent.txt"), "changed");
  await assert.rejects(buildEvents(root, manifest), /digest mismatch/);
  manifest.documents[0].file = join(outside, "outside.txt");
  await assert.rejects(buildEvents(root, manifest), /outside the fixture root/);
});
