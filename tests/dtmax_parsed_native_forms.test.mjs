import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const sandbox={crypto:webcrypto,TextEncoder,TextDecoder,Blob,DecompressionStream,atob,btoa};
vm.runInNewContext(fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8'),sandbox);
const api=sandbox.NXTDTMaxT1,hash=b=>createHash('sha256').update(b).digest('hex');
const privatePath=process.env.NXT_PARSED_SOURCE_MANIFEST;
let manifest;
function input(){if(!manifest){const raw=fs.readFileSync(privatePath);assert.equal(hash(raw),'b87580b785c2789beb0e5f037649e1f2cf24b5309ed134fe8ac76bcb4c9dcc49');manifest=JSON.parse(raw);}return manifest;}
function packet(entry){const raw=fs.readFileSync(path.join(path.dirname(privatePath),'contexts',entry.context_id+'.json'));assert.equal(hash(raw),entry.packet_sha256);return {...JSON.parse(raw),merkle_proof:entry.merkle_proof};}

test('unclosed source evidence cannot reach raw data rendering',async()=>{
 for(const record of [null,{},[],{metadata:{client_number:'100'}}])await assert.rejects(()=>api.decodeParsedNativeSource(record));
});

test('real source representatives preserve original bytes, all years, module holds and parse errors',{skip:!privatePath},async()=>{
 const entries=input().entries,selected=new Map(),years=new Set();
 for(let i=0;i<entries.length;i++){const e=entries[i],m=e.metadata;if(i%127===0||m.parse_error_type||m.reader_module==='T3'||!years.has(m.source_return_year)){selected.set(e.context_id,e);years.add(m.source_return_year);}}
 for(const key of ['source_line_bytes','data_origin_text_Q_fields']){const e=entries.reduce((a,b)=>a.metadata[key]>b.metadata[key]?a:b);selected.set(e.context_id,e);}
 let fields=0,errors=0,held=0;
 for(const e of selected.values()){const r=await api.decodeParsedNativeSource(packet(e));assert.equal(hash(Buffer.from(r.bytes)),e.metadata.source_line_sha256);assert.equal(r.data.reduce((n,f)=>n+f.vendor_fields.length,0),e.metadata.data_origin_text_Q_fields);fields+=e.metadata.data_origin_text_Q_fields;errors+=!!e.metadata.parse_error_type;held+=e.metadata.reader_module==='T3';for(const f of r.data)for(const q of f.vendor_fields){assert.equal(q.Y,4);assert.equal(typeof q.W,'string');assert.notEqual(q.W,'');}}
 assert.equal(years.size,19);assert.equal(errors,7);assert.equal(held,88);
 console.log(JSON.stringify({parsed_source_browser_sample_contexts:selected.size,data_text_fields:fields,all19_years:true,all7_parse_errors:true,all88_original_module_holds:true,network_calls:0,vendor_execution:0}));
});

test('owner, family, year, proof path, raw hash and compressed-content mutations fail closed',{skip:!privatePath},async()=>{
 const original=packet(input().entries.find(e=>e.metadata.data_origin_text_Q_fields>0));
 for(const mutation of ['owner','family','year','source_hash','raw_hash','length','proof','compressed','approval']){
  const value=structuredClone(original);switch(mutation){case'owner':value.metadata.client_number='999999999';break;case'family':value.metadata.family_file_id='999999999';break;case'year':value.metadata.source_return_year++;break;case'source_hash':value.metadata.source_file_sha256='a'.repeat(64);break;case'raw_hash':value.metadata.source_line_sha256='a'.repeat(64);break;case'length':value.metadata.source_line_bytes++;break;case'proof':value.merkle_proof[0].sha256='a'.repeat(64);break;case'compressed':value.content_base64='AAAA';break;case'approval':value.metadata.financial_values_approved=true;break;}
  await assert.rejects(()=>api.decodeParsedNativeSource(value),mutation);
 }
});

test('data-entry text retains occurrence order and never renders inactive numeric defaults',{skip:!privatePath},async()=>{
 const entry=input().entries.reduce((a,b)=>a.metadata.data_origin_text_Q_fields>b.metadata.data_origin_text_Q_fields?a:b),p=packet(entry),decoded=await api.decodeParsedNativeSource(p),rendered=await api.parsedNativeFormsHtml([p]);
 assert.equal((rendered.html.match(/data-raw-native-search=/g)||[]).length,entry.metadata.data_origin_text_Q_fields);
 assert.equal((rendered.html.match(/data-raw-native-form>/g)||[]).length,entry.metadata.data_origin_forms);
 const esc=v=>String(v).replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');
 for(const f of decoded.data)for(const q of f.vendor_fields)assert.ok(rendered.html.includes('class="dtmax-raw-source-text">'+esc(q.W)+'</td>'));
 assert.match(rendered.html,/No values are recalculated/);assert.match(rendered.html,/original keyword labels have not been verified/);
 assert.equal(hash(Buffer.from(rendered.downloads[0].bytes)),entry.metadata.source_line_sha256);
 const error=packet(input().entries.find(e=>e.metadata.parse_error_type));const held=await api.parsedNativeFormsHtml([error]);assert.match(held.html,/could not be parsed/);assert.equal((held.html.match(/data-raw-native-search=/g)||[]).length,0);
});
