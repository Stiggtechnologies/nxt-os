import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const sha=b=>createHash('sha256').update(b).digest('hex'),canonical=j=>JSON.stringify(Object.fromEntries(Object.keys(j).sort().map(k=>[k,j[k]])));
const sandbox={crypto:webcrypto,Uint8Array,TextEncoder,TextDecoder,Blob,DecompressionStream,atob,btoa};vm.runInNewContext(fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8'),sandbox);const api=sandbox.NXTDTMaxT1;
const raw=fs.readFileSync(new URL('../docs/dtmax-empty-export-classifications.json',import.meta.url)),closed=JSON.parse(raw);
test('diagnostic inventory stays closed to five empty and two control-only saved export contexts',()=>{
 assert.equal(sha(raw),'00d819b97dcee25ce7109bdedd2c69c4279c0525d3ac76fe12b9ea37d4a2b1b2');assert.equal(closed.records.length,7);assert.equal(closed.counts.export_without_individual_EXR,5);assert.equal(closed.counts.export_control_only_type8,2);assert.equal(closed.contains_source_client_family_ids_names_SIN_DOB_or_financial_values,false);
 for(const r of closed.records){assert.equal(r.individual_Q_values_recovered,0);assert.equal(r.native_source_absence_established,false);assert.equal(r.original_parse_error_type,'System.Collections.Generic.KeyNotFoundException');assert.equal(r.original_parsed_packet_unchanged,true);}
});
test('unknown or incomplete metadata cannot acquire a verified export classification',async()=>{
 for(const value of [null,{},[],{source_line_sha256:'a'.repeat(64)},{source_line_sha256:closed.records[0].source_line_sha256,parse_error_type:'System.Collections.Generic.KeyNotFoundException'}])assert.equal(await api.sourceContextDiagnostic(value),'');
});
const privatePath=process.env.NXT_PARSED_SOURCE_MANIFEST;
test('synthetic unknown exception metadata keeps its original unclassified state',async()=>{
 const r={source_archive_sha256:'a'.repeat(64),source_archive_bytes:1000,source_context_offset:100,source_context_sha256:'b'.repeat(64),source_context_bytes:70,source_year_frame_offset:56,source_year_frame_sha256:'c'.repeat(64),family_file_id:'99',client_number:'100',tax_year:2024,reason:'missing_identity_information',frame_tags:['EXC','EXP'],immutable:true,identity_metadata_readable:false,normalized_amounts_created:false,header_ids_are_source_references_only:true};
 const html=await api.exceptionsWithDiagnosticsHtml([r]);assert.match(html,/Identity information missing/);assert.doesNotMatch(html,/data-export-context-class/);
});
test('existing exception list shows exactly seven source explanations and keeps all eight original rows',{skip:!privatePath},async()=>{
 const raw=fs.readFileSync(path.join(path.dirname(privatePath),'../source-exception-hosted-readback-private.json'));assert.equal(sha(raw),'1eb68981b55f61942a723b5bf1aa5392a74d31082e12c93a2a5596987a4a5378');const rows=JSON.parse(raw),html=await api.exceptionsWithDiagnosticsHtml(rows);
 assert.equal((html.match(/data-export-context-class=/g)||[]).length,7);assert.equal((html.match(/data-exception-download=/g)||[]).length,8);assert.equal((html.match(/export_without_individual_EXR/g)||[]).length,5);assert.equal((html.match(/export_control_only_type8/g)||[]).length,2);assert.match(html,/not absence from the native database/);
 for(const original of rows){const r=structuredClone(original);r.client_number+='0';assert.doesNotMatch(await api.exceptionsWithDiagnosticsHtml([r]),/data-export-context-class/);}
});
test('all seven original packets retain their error and original bytes with exact bounded diagnostics',{skip:!privatePath},async()=>{
 const manifestRaw=fs.readFileSync(privatePath);assert.equal(sha(manifestRaw),'b87580b785c2789beb0e5f037649e1f2cf24b5309ed134fe8ac76bcb4c9dcc49');const manifest=JSON.parse(manifestRaw);let count=0;
 for(const d of closed.records){const e=manifest.entries.find(e=>e.context_id===d.source_context_id);assert.ok(e);const packetRaw=fs.readFileSync(path.join(path.dirname(privatePath),'contexts',e.context_id+'.json'));assert.equal(sha(packetRaw),d.source_packet_sha256);const packet={...JSON.parse(packetRaw),merkle_proof:e.merkle_proof};const decoded=await api.decodeParsedNativeSource(packet);assert.equal(sha(Buffer.from(canonical(decoded.metadata))),d.source_metadata_sha256);
  const html=await api.parsedNativeFormsHtml([packet]);assert.ok(html.html.includes('data-export-context-class="'+d.classification+'"'));assert.match(html.html,/does not establish that data is absent from the native database/);assert.match(html.html,/System.Collections.Generic.KeyNotFoundException/);assert.doesNotMatch(html.html,/data-raw-native-search=/);assert.equal(sha(Buffer.from(html.downloads[0].bytes)),d.source_line_sha256);assert.equal(decoded.data.length,0);
  for(const key of ['client_number','family_file_id','source_return_year','source_line_number','source_file_sha256','source_line_bytes']){const m=structuredClone(decoded.metadata);m[key]=typeof m[key]==='number'?m[key]+1:m[key]+'0';assert.equal(await api.sourceContextDiagnostic(m),'',key);}
  count++;
 }
 assert.equal(count,7);
 const valid=manifest.entries.find(e=>!e.metadata.parse_error_type&&e.metadata.data_origin_text_Q_fields>0);assert.equal(await api.sourceContextDiagnostic(valid.metadata),'');
});
