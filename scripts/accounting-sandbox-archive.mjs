
import { createHash } from "node:crypto";
import { readFile, realpath, stat } from "node:fs/promises";
import { relative, resolve, isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";

const model = "sandbox-token-hash-v1";
const dimensions = 128;
const maxFileBytes = 96 * 1024;
const safeId = /^[a-zA-Z0-9_-]{1,96}$/;
const sha = data => createHash("sha256").update(data).digest("hex");

export function embed(text) {
  const values = Array(dimensions).fill(0);
  for (const token of text.normalize("NFKC").toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []) {
    const digest = createHash("sha256").update(token).digest();
    values[digest[0] % dimensions] += digest[1] % 2 ? 1 : -1;
  }
  return values.some(Boolean) ? { model, values } : undefined;
}

function sliceText(text) {
  const parts = [];
  let chunk = "";
  let bytes = 0;
  for (const char of text) {
    const size = Buffer.byteLength(char);
    if (bytes + size > 3_000) {
      parts.push(chunk);
      chunk = "";
      bytes = 0;
    }
    chunk += char;
    bytes += size;
  }
  if (chunk) parts.push(chunk);
  return parts.map((part, index) => ({ locator: `sandbox segment ${index + 1}`, text: part, embedding: embed(part) }))
    .filter(chunk => chunk.text.trim());
}

function date(value) {
  return typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value)
    && Number.isFinite(Date.parse(value)) && new Date(value).toISOString().slice(0, 10) === value;
}

export async function buildEvents(root, manifest) {
  if (manifest?.classification !== "synthetic_sandbox_only" ||
      !Array.isArray(manifest.documents) || !manifest.documents.length || manifest.documents.length > 16 ||
      !Number.isSafeInteger(manifest.after) || manifest.after < 0 || manifest.after > Number.MAX_SAFE_INTEGER - 17 ||
      typeof manifest.permissions_observed_at !== "string" ||
      !Number.isFinite(Date.parse(manifest.permissions_observed_at))) {
    throw new Error("Invalid synthetic archive manifest");
  }
  const base = await realpath(root);
  const events = [];
  const ids = new Set();
  let seq = manifest.after;
  for (const doc of manifest.documents) {
    if (!safeId.test(doc.id) || ids.has(doc.id) || !safeId.test(doc.version) ||
        typeof doc.file !== "string" || !/\.txt$/i.test(doc.file) ||
        !/^[0-9a-f]{64}$/.test(doc.sha256) || !date(doc.from) || !date(doc.through) ||
        doc.from > doc.through || !Array.isArray(doc.user_ids) || !doc.user_ids.length ||
        !doc.user_ids.every(id => safeId.test(id) && id.startsWith("usr_"))) {
      throw new Error("Invalid synthetic archive document");
    }
    ids.add(doc.id);
    const file = await realpath(resolve(base, doc.file));
    const local = relative(base, file);
    if (!local || local.startsWith("..") || isAbsolute(local)) throw new Error("Archive file is outside the fixture root");
    const info = await stat(file);
    if (!info.isFile() || info.size > maxFileBytes) throw new Error("Invalid synthetic archive file");
    const bytes = await readFile(file);
    if (bytes.length > maxFileBytes || sha(bytes) !== doc.sha256) throw new Error("Synthetic archive digest mismatch");
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/u.test(text)) throw new Error("Unexpected binary data in text fixture");
    const chunks = sliceText(text);
    if (!chunks.length) throw new Error("Empty synthetic archive document");
    if (sha(chunks.map(chunk => chunk.text).join("")) !== doc.sha256) {
      throw new Error("Synthetic archive chunks cannot reproduce the original UTF-8 digest");
    }
    events.push({ sequence: ++seq, change: { kind: "upsert", document: {
      id: doc.id, version: doc.version, content_sha256: doc.sha256,
      from: doc.from, through: doc.through, user_ids: doc.user_ids, chunks,
    } } });
  }
  events.push({ sequence: ++seq, change: {
    kind: "permissions_observed", observed_at: new Date(manifest.permissions_observed_at).toISOString(),
  } });
  return events;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv[2] === "--query" && process.argv[3]) {
      const result = embed(process.argv[3]);
      if (!result) throw new Error("Query has no embedding tokens");
      console.log(JSON.stringify(result));
    } else {
      const [, , root, manifestPath] = process.argv;
      if (!root || !manifestPath) throw new Error("Pass a synthetic fixture root and manifest");
      const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
      for (const event of await buildEvents(root, manifest)) console.log(JSON.stringify(event));
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Synthetic archive job failed");
    process.exitCode = 1;
  }
}
