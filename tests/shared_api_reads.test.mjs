import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
const source = fs.readFileSync(new URL("../public/nxt/ui.js", import.meta.url), "utf8");
const response = value => ({ ok: true, status: 200, json: async () => value });
function load(fetch, parent, sessionStorage = { value: "staff-a", getItem() { return this.value; }, setItem(k,v) { this.value=v; }, removeItem() { this.value=""; } }) {
  const sb = { fetch, sessionStorage, AbortController, structuredClone, URL, setTimeout, clearTimeout, location: { origin: "https://nxt.invalid" }, document: {} };
  sb.window = sb; sb.parent = parent || sb;
  vm.runInNewContext(source, sb);
  return sb;
}
test("concurrent shell and iframe GETs share transport but not mutable response objects", async () => {
  let release, calls = 0;
  const fetch = () => { calls++; return new Promise(r => release=r); };
  const shell = load(fetch), frame = load(fetch, shell, shell.sessionStorage);
  const a = shell.NXT.api("/api/clients"), b = frame.NXT.api("/api/clients");
  assert.equal(calls, 1);
  release(response({ clients: [{ name: "Synthetic" }] }));
  const [one,two] = await Promise.all([a,b]);
  one.clients[0].name = "Changed";
  assert.equal(two.clients[0].name, "Synthetic");
});
test("one caller can abort without cancelling another subscriber", async () => {
  let release, signal;
  const sb = load((p,init) => { signal=init.signal; return new Promise(r=>release=r); });
  const controller = new AbortController();
  const a = sb.NXT.api("/api/clients", { signal: controller.signal });
  const rejected = assert.rejects(a, { name: "AbortError" });
  const b = sb.NXT.api("/api/clients");
  controller.abort(); await rejected;
  assert.equal(signal.aborted, false);
  release(response({ clients: [] })); await b;
});
test("last cancellation aborts transport and next caller gets a fresh request", async () => {
  let calls = 0;
  const sb = load((p,init) => {
    calls++;
    if (calls > 1) return Promise.resolve(response({ fresh: true }));
    return new Promise((r,reject) => init.signal.addEventListener("abort", () => reject(Object.assign(new Error("abort"), { name: "AbortError" }))));
  });
  const controller = new AbortController(), a = sb.NXT.api("/api/clients", {signal:controller.signal});
  const rejected = assert.rejects(a, {name:"AbortError"}); controller.abort(); await rejected;
  assert.equal((await sb.NXT.api("/api/clients")).fresh, true);
  assert.equal(calls, 2);
});
test("only catalog metadata is reused after completion; financial reads are fresh", async () => {
  const calls = [];
  const sb = load(async p => { calls.push(p); return response({n:calls.length}); });
  await sb.NXT.api("/api/tax/catalog"); await sb.NXT.api("/api/tax/catalog");
  assert.equal(calls.length, 1);
  await sb.NXT.api("/api/ledger/reports/dashboard"); await sb.NXT.api("/api/ledger/reports/dashboard");
  assert.equal(calls.length, 3);
  await sb.NXT.api("/api/tax/returns", {body:{name:"Synthetic"}});
  await sb.NXT.api("/api/tax/catalog");
  assert.equal(calls.filter(p=>p==="/api/tax/catalog").length, 2);
});
test("sign-out discards pending data and prevents sharing between identities", async () => {
  let release, calls=0;
  const sb = load(async () => { calls++; if(calls===1) return new Promise(r=>release=r); return response({identity:"new"}); });
  const old = sb.NXT.api("/api/staff/me"), rejected=assert.rejects(old,{name:"AbortError"});
  sb.NXT.key.set("staff-b");
  assert.equal((await sb.NXT.api("/api/staff/me")).identity, "new");
  release(response({identity:"old"})); await rejected;
  assert.equal(calls,2);
});
test("failed requests are evicted and never reused as successful metadata", async () => {
  let calls=0;
  const sb=load(async()=>{calls++;return calls===1?{ok:false,status:503,json:async()=>({error:"temporarily unavailable"})}:response({ok:true});});
  await assert.rejects(sb.NXT.api("/api/tax/catalog"),/temporarily unavailable/);
  assert.equal((await sb.NXT.api("/api/tax/catalog")).ok,true);
  assert.equal(calls,2);
});
