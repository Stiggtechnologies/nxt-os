
import { createReadStream } from "node:fs";
import readline from "node:readline";
import { pathToFileURL } from "node:url";

const identifier = /^[a-zA-Z0-9_-]{1,96}$/;
const maxEventBytes = 512 * 1024;

export async function syncEvents({ origin, token, company, source, input, after = 0, send = fetch }) {
  const url = new URL(origin);
  if (
    url.protocol !== "https:" || url.username || url.password || url.search || url.hash
    || url.pathname !== "/" || !identifier.test(company) || !identifier.test(source)
    || !Number.isSafeInteger(after) || after < 0 || !token || /\s/.test(token)
  ) {
    throw new Error("Invalid archive sync configuration");
  }
  url.pathname = `/api/accounting-extension/archive/${company}/${source}/events`;
  const lines = readline.createInterface({ input, crlfDelay: Infinity });
  let sequence = after;
  let previous = 0;
  for await (const line of lines) {
    if (!line.trim()) continue;
    if (Buffer.byteLength(line) > maxEventBytes) throw new Error("Archive event exceeds the request limit");
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      throw new Error("Archive manifest contains invalid JSON");
    }
    if (!Number.isSafeInteger(event?.sequence) || event.sequence <= previous) {
      throw new Error("Archive manifest sequence must strictly increase");
    }
    previous = event.sequence;
    if (event.sequence <= after) continue;
    if (event.sequence !== sequence + 1) throw new Error("Archive manifest contains a sequence gap");

    let response;
    try {
      response = await send(url, {
        method: "POST",
        redirect: "error",
        signal: AbortSignal.timeout(30_000),
        headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
        body: line,
      });
    } catch {
      throw new Error(`No acknowledgment for archive event ${event.sequence}; verify the sequence before continuing`);
    }
    if (!response.ok) {
      throw new Error(`Archive event ${event.sequence} was rejected (HTTP ${response.status})`);
    }
    const receipt = await response.json().catch(() => null);
    if (receipt?.sequence !== event.sequence) {
      throw new Error(`Archive acknowledgment did not match event ${event.sequence}`);
    }
    sequence = event.sequence;
  }
  return sequence;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [, , company, source, manifest, after = "0"] = process.argv;
  if (!manifest || !/^\d+$/.test(after)) {
    console.error("Usage: node scripts/accounting-archive-sync.mjs COMPANY SOURCE MANIFEST [LAST_ACKNOWLEDGED_SEQUENCE]");
    process.exitCode = 1;
  } else {
    try {
      const sequence = await syncEvents({
        origin: process.env.NXT_ACCOUNTING_ARCHIVE_ORIGIN,
        token: process.env.NXT_ACCOUNTING_ARCHIVE_STAFF_TOKEN,
        company, source, input: createReadStream(manifest), after: Number(after),
      });
      console.log(JSON.stringify({ sequence }));
    } catch (error) {
      console.error(error instanceof Error ? error.message : "Archive synchronization failed");
      process.exitCode = 1;
    }
  }
}
