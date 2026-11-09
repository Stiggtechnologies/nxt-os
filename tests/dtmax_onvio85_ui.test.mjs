import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import{createHash,webcrypto}from'node:crypto';
import{gunzipSync}from'node:zlib';
const main=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8'),sdk=fs.readFileSync(new URL('../public/apps/tax/dtmax-onvio85.js',import.meta.url),'utf8'),profile=JSON.parse(fs.readFileSync(new URL('../public/apps/tax/dtmax-archived-extract-profile.json',import.meta.url))),hash=b=>createHash('sha256').update(b).digest('hex');
const packets=process.env.NXT_ARCHIVED_EXTRACT_PACKETS?fs.readFileSync(process.env.NXT_ARCHIVED_EXTRACT_PACKETS,'utf8').trim().split('\n').map(JSON.parse):null;
const fixture=process.env.NXT_ONVIO85_FIXTURE?JSON.parse(fs.readFileSync(process.env.NXT_ONVIO85_FIXTURE)):null;
function environment(mode='valid'){
 let loads=0,script;const sandbox={crypto:webcrypto,TextEncoder,TextDecoder,Uint8Array,DataView,atob,Blob,Response,DecompressionStream,document:{createElement:tag=>{assert.equal(tag,'script');return{};},head:{appendChild:s=>{loads++;script=s;if(mode==='error'){s.onerror();return;}vm.runInContext(mode==='invalid'?'globalThis.NXTOnvio85={}':sdk,context);s.onload();}}}},context=vm.createContext(sandbox);vm.runInContext(main,context);return{api:sandbox.NXTDTMaxT1,get script(){return script;},get loads(){return loads;}};
}
test('SDK loader uses the single same-origin classic asset with exact SRI and fails closed',async()=>{
 const e=environment();const loaded=await e.api.loadOnvio85SDK();assert.equal(loaded.interpretationAuthority.record_count,85);assert.equal(e.loads,1);assert.equal(e.script.src,'/apps/tax/dtmax-onvio85.js');assert.equal(e.script.integrity,'sha256-JMCc5z2tiPOyLPFFwOeSQVBZwMdYX+AK7KehVto28AA=');assert.equal(e.script.crossOrigin,'anonymous');assert.equal(e.script.referrerPolicy,'no-referrer');assert.equal(await e.api.loadOnvio85SDK(),loaded);assert.equal(e.loads,1);
 for(const mode of ['error','invalid'])await assert.rejects(()=>environment(mode).api.loadOnvio85SDK());
 assert.ok(!/\beval\s*\(|new Function\s*\(|createObjectURL\s*\([^)]*script/.test(sdk));
});
test('all448 original packets keep195 holds and derive only85 exact separate downloads',{skip:!packets||!fixture},async()=>{
 const e=environment(),canonical=e.api.canonicalRecoveredSource,expected=new Map(fixture.expected_records.map(r=>[hash(canonical(r)),canonical(r)]));let observations=0,oldHeld=0,zeros=0,absent=0,fields=0;
 for(const packet of packets){const before=canonical(packet),view=await e.api.archivedSourcesHtml([packet],{profile,inflate:gunzipSync,loadKeywords:async()=>null});assert.equal(Buffer.from(view.downloads[0].packetBytes).toString(),before);assert.equal(canonical(packet),before);oldHeld+=(view.html.match(/data-archived-record-held/g)||[]).length;assert.equal((view.html.match(/data-onvio85-observation=/g)||[]).length,view.additionalDownloads.length);
  for(const d of view.additionalDownloads){observations++;assert.equal(Buffer.from(d.bytes).toString(),expected.get(d.sha256));assert.equal(hash(d.bytes),d.sha256);assert.equal(d.record.old_frozen_grammar_holds_modified,false);assert.equal(d.record.preferred_version_selection,false);assert.ok(view.html.includes('The original calculation record remains held in the frozen packet'));assert.ok(view.html.includes('Separately verified additional requested-line observations appear below'));assert.ok(!view.html.includes('These entire records are download-only; their forms and values are not displayed')); for(const f of d.record.E11_forms)for(const entries of Object.values(f.lines)){if(entries===null){absent++;assert.ok(view.html.includes('Not present in this archived E11'));}else for(const q of entries){fields++;zeros+=q.wire_cents===0;assert.ok(view.html.includes(e.api.cents(String(q.wire_cents))));}}}
 }
 assert.equal(e.loads,1);assert.equal(observations,85);assert.equal(oldHeld,195);assert.equal(fields,410);assert.equal(zeros,61);assert.equal(absent,100);
});
test('SDK unavailability preserves original download and suppresses all additional values',{skip:!packets||!fixture},async()=>{
 const e=environment('error'),offset=fixture.expected_records[0].source_context_offset,packet=packets.find(r=>r.source_locator.source_context_offset===offset),view=await e.api.archivedSourcesHtml([packet],{profile,inflate:gunzipSync,loadKeywords:async()=>null});assert.equal(view.additionalDownloads.length,0);assert.ok(view.html.includes('This additional interpretation is unavailable'));assert.ok(!view.html.includes('data-onvio85-observation='));assert.ok(view.html.includes('entire record is held for download only'));assert.equal(Buffer.from(view.downloads[0].packetBytes).toString(),e.api.canonicalRecoveredSource(packet));
});
test('the optional section does not change default totals, original records or source assignment APIs',()=>{
 const defaults=main.slice(main.indexOf('function defaultSourceKind()'),main.indexOf('function bindClient()'));assert.ok(!defaults.includes('onvio85'));assert.ok(!defaults.includes('archived-extract'));assert.ok(main.includes('bindOnvio85Downloads(detail,rendered)'));assert.ok(sdk.includes('old_frozen_grammar_holds_modified:false'));assert.ok(!sdk.includes('fetch('));assert.ok(!sdk.includes('localStorage'));assert.ok(!sdk.includes('sessionStorage'));
});
