
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const maxBytes = 8 * 1024 * 1024;
const idPattern = /^[a-zA-Z0-9_-]{1,96}$/;
const hashPattern = /^[a-f0-9]{64}$/;

export async function observeSources(registry, { send = fetch, now = () => new Date() } = {}) {
  if (!Array.isArray(registry) || registry.length > 100) throw new Error("Invalid authority registry");
  const seen = new Set();
  const entries = registry.map(entry => {
    const url = new URL(entry.url);
    if (!idPattern.test(entry.id) || seen.has(entry.id) || !idPattern.test(entry.jurisdiction)
        || (entry.previous_sha256 != null && !hashPattern.test(entry.previous_sha256))
        || url.protocol !== "https:" || url.username || url.password || url.hash) {
      throw new Error("Invalid authority registry entry");
    }
    seen.add(entry.id);
    return { entry, url };
  });
  const observations = [];
  for (const { entry, url } of entries) {
    try {
      const signal = AbortSignal.timeout(30_000);
      const response = await send(url, { redirect: "error", signal });
      if (!response.ok || !response.body) throw new Error("Source response unavailable");
      const hash = createHash("sha256");
      let size = 0;
      for await (const chunk of response.body) {
        size += chunk.byteLength;
        if (size > maxBytes) throw new Error("Source exceeds the byte limit");
        hash.update(chunk);
      }
      const observed = hash.digest("hex");
      observations.push({
        source_id: entry.id,
        jurisdiction: entry.jurisdiction,
        url: url.href,
        observed_at: now().toISOString(),
        source_sha256: observed,
        status: observed === entry.previous_sha256 ? "unchanged" : "review_required",
      });
    } catch {
      observations.push({ source_id: entry.id, jurisdiction: entry.jurisdiction, status: "unavailable" });
    }
  }
  return observations;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (!process.argv[2]) throw new Error("Pass the firm-approved public authority registry JSON");
    const registry = JSON.parse(readFileSync(process.argv[2], "utf8"));
    const observations = await observeSources(registry);
    console.log(JSON.stringify(observations, null, 2));
    if (observations.some(item => item.status === "unavailable")) process.exitCode = 2;
  } catch {
    console.error("Could not read the approved public authority registry");
    process.exitCode = 1;
  }
}
