import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createHash,webcrypto} from 'node:crypto';
const sandbox={crypto:webcrypto,Uint8Array,TextEncoder,TextDecoder,AbortSignal,Blob,DecompressionStream,atob,btoa};
vm.runInNewContext(fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8'),sandbox);
const api=sandbox.NXTDTMaxT1,sha=b=>createHash('sha256').update(b).digest('hex');
const provenance=JSON.parse(fs.readFileSync(new URL('../docs/dtmax-installed-keyword-provenance.json',import.meta.url)));
const bytes=year=>fs.readFileSync(new URL('../public/apps/tax/dtmax-keywords/'+year+'.json',import.meta.url));
test('all 19 exact-year dictionaries are pinned and retain all 47,692 original keyword names',async()=>{
 let count=0;const years=[];
 for(const d of provenance.dictionaries){const raw=bytes(d.year),table=await api.verifySourceKeywordTable(d.year,new Uint8Array(raw));assert.equal(sha(raw),d.sha256);assert.equal(table.carrier_sha256,d.carrier_sha256);assert.equal(Object.keys(table.keywords).length,d.keywords);assert.equal(table.keywords['7004'],'Authorize');count+=d.keywords;years.push(d.year);assert.ok(Object.isFrozen(table.keywords));}
 assert.equal(count,47692);assert.deepEqual(years,[2005,...Array.from({length:18},(_,i)=>2009+i)]);assert.equal(provenance.client_data_included,false);
});
test('changed bytes, swapped years, unsupported years and unavailable fetch cannot invent labels',async()=>{
 const raw=new Uint8Array(bytes(2023));raw[raw.length-2]^=1;await assert.rejects(()=>api.verifySourceKeywordTable(2023,raw));await assert.rejects(()=>api.verifySourceKeywordTable(2024,new Uint8Array(bytes(2023))));
 assert.equal(await api.loadSourceKeywords(2006,()=>{throw Error('must not fetch another year');}),null);
 assert.equal(await api.loadSourceKeywords(2023,async()=>({ok:false})),null);
 assert.equal(await api.loadSourceKeywords(2023,async()=>({ok:true,arrayBuffer:async()=>bytes(2024)})),null);
 const requests=[];const table=await api.loadSourceKeywords(2023,async(url,options)=>{requests.push({url,options});return{ok:true,arrayBuffer:async()=>bytes(2023)};});
 assert.equal(table.year,2023);assert.equal(requests.length,1);assert.equal(requests[0].url,'/apps/tax/dtmax-keywords/2023.json');assert.equal(requests[0].options.credentials,'omit');assert.equal(requests[0].options.redirect,'error');
});
test('keyword namespace never renames native form codes, held modules or mismatched years',async()=>{
 const table=await api.verifySourceKeywordTable(2023,new Uint8Array(bytes(2023)));
 assert.equal(api.keywordName({reader_module:'T1',source_return_year:2023},7004,table),'Authorize');
 for(const m of [{reader_module:'T3',source_return_year:2023},{reader_module:'T1',source_return_year:2024},{reader_module:'T2',source_return_year:2023}])assert.equal(api.keywordName(m,7004,table),'');
 assert.equal(api.keywordName({reader_module:'T1',source_return_year:2023},65535,table),'');
});
const privatePath=process.env.NXT_PARSED_SOURCE_MANIFEST;
test('real source fields gain labels without changing values, source form176 or original downloads',{skip:!privatePath},async()=>{
 const raw=fs.readFileSync(privatePath);assert.equal(sha(raw),'b87580b785c2789beb0e5f037649e1f2cf24b5309ed134fe8ac76bcb4c9dcc49');const manifest=JSON.parse(raw);
 const packet=e=>({...JSON.parse(fs.readFileSync(path.join(path.dirname(privatePath),'contexts',e.context_id+'.json'))),merkle_proof:e.merkle_proof});
 const loader=async year=>api.verifySourceKeywordTable(year,new Uint8Array(bytes(year)));
 const selected=manifest.entries.filter(e=>e.metadata.reader_module==='T1'&&e.metadata.data_origin_text_Q_fields>0);let found=false;
 for(const entry of selected.slice(0,50)){
  const p=packet(entry),decoded=await api.decodeParsedNativeSource(p);if(!decoded.data.some(f=>f.vendor_form==='176'&&f.vendor_fields.some(q=>q.z===7004)))continue;
  const html=await api.parsedNativeFormsHtml([p],{loadKeywords:loader});assert.match(html.html,/Source form 176/);assert.match(html.html,/Authorize<small>Field ID 7004/);assert.match(html.html,/Installed DT Max T1 keyword labels/);assert.equal(sha(Buffer.from(html.downloads[0].bytes)),entry.metadata.source_line_sha256);assert.equal((html.html.match(/data-raw-native-search=/g)||[]).length,entry.metadata.data_origin_text_Q_fields);found=true;break;
 }
 assert.equal(found,true);
 const held=manifest.entries.find(e=>e.metadata.reader_module==='T3'&&e.metadata.data_origin_text_Q_fields>0);let loads=0;const html=await api.parsedNativeFormsHtml([packet(held)],{loadKeywords:async()=>{loads++;throw Error('held module must not load T1 names');}});assert.equal(loads,0);assert.match(html.html,/Original reader module T3/);assert.doesNotMatch(html.html,/Installed DT Max T1 keyword labels/);
});
