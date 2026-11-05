import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
const src = fs.readFileSync(new URL("../public/apps/nixon/start.js", import.meta.url), "utf8");
test("start link keeps only source, utm and click-id params", () => {
  const sb = { URLSearchParams };
  vm.runInNewContext(src, sb);
  const { entryFrom } = sb.NixonStart;
  assert.deepEqual({ ...entryFrom("?utm_source=facebook&utm_campaign=t1&email=a@b.c&gclid=x") }, { utm_source: "facebook", utm_campaign: "t1", gclid: "x" });
  assert.deepEqual({ ...entryFrom("") }, {});
});
test("start page is noindex and Nixon-branded, not TurboTax", () => {
  const html = fs.readFileSync(new URL("../public/apps/nixon/start.html", import.meta.url), "utf8");
  assert.match(html, /noindex/); assert.match(html, /Nixon/); assert.doesNotMatch(html, /turbotax/i);
});
