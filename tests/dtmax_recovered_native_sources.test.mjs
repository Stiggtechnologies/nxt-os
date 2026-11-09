import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const profileBytes=fs.readFileSync(new URL('../public/apps/tax/dtmax-recovered-native-profile.json',import.meta.url)),profile=JSON.parse(profileBytes);
const sandbox={crypto:webcrypto,TextEncoder,TextDecoder,Uint8Array,atob,Blob,fetch:async(url,options)=>{assert.equal(url,'/apps/tax/dtmax-recovered-native-profile.json');assert.equal(options.credentials,'omit');assert.equal(options.redirect,'error');return {ok:true,arrayBuffer:async()=>profileBytes};}};
vm.runInNewContext(fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8'),sandbox);
const api=sandbox.NXTDTMaxT1,path=process.env.NXT_RECOVERED_NATIVE_BATCH;
const batch=path?JSON.parse(fs.readFileSync(path)):null,sha=b=>createHash('sha256').update(b).digest('hex');
test('the complete published profile passes the real byte-length and SHA loading gate',async()=>{
 const loaded=await api.loadRecoveredNativeProfile();assert.equal(profileBytes.length,68309);assert.equal(loaded.profile_sha256,'fc858ae42305620f2949805b9994c196949478c26a085d8884c1e988c289ab93');assert.equal(loaded.context_sha256.length,80);
});
test('unclosed native contexts and invented values are rejected',async()=>{
 for(const value of [null,{},[],{schema:'recovered-native-source-context/v1'}])await assert.rejects(()=>api.verifyRecoveredNativeContext(value,profile));
 assert.equal(api.recoveredSourceValue({Y:2,exact_cents:'0'}),'$0.00');
 assert.equal(api.recoveredSourceValue({Y:2,exact_cents:'-1'}),'-$0.01');
 assert.equal(api.recoveredSourceValue({Y:4,W:''}),'(empty)');
 assert.equal(api.recoveredSourceValue({Y:4,W:'\v<source>'}),'\v<source>');
});
test('all actual recovered bodies are hash-exact with distinct client namespaces',{skip:!batch},async()=>{
 assert.equal(batch.contexts.length,80);let records=0,assigned=0;
 for(const context of batch.contexts){const verified=await api.verifyRecoveredNativeContext(context,profile);assert.ok(profile.context_sha256.includes(verified.digest));assert.equal(JSON.parse(Buffer.from(verified.bytes)).context_id,context.context_id);records+=context.records.length;assigned+=context.client_number!==null;}
 assert.equal(records,286);assert.equal(assigned,38);
});
test('client, year, plan, amounts, approval and body mutations fail closed',{skip:!batch},async()=>{
 const original=batch.contexts.find(c=>c.records.some(r=>r.typed?.state==='parsed'));
 const mutations=[c=>c.client_number='999999999',c=>c.source_year++,c=>c.source_identity.source_native_client_id='999999999',c=>c.financial_values_approved=true,c=>c.records[0].source.source_plan++,c=>c.records[0].body_base64='AAAA',c=>c.records[0].body_sha256='a'.repeat(64),c=>c.records.pop()];
 for(const mutate of mutations){const c=structuredClone(original);mutate(c);await assert.rejects(()=>api.verifyRecoveredNativeContext(c,profile));}
});
test('actual source renderer preserves all fields and exact E11 instances without calc keyword labels',{skip:!batch},async()=>{
 let fields=0,instances=0,forms=0,unassigned=0;
 for(const c of batch.contexts){
  const view=await api.recoveredNativeSourcesHtml([c],{profile,loadKeywords:async()=>null});assert.equal(view.downloads.length,1);assert.equal(view.downloads[0].packet.context_id,c.context_id);
  const fieldCount=(view.html.match(/data-recovered-search=/g)||[]).length;fields+=fieldCount;forms+=(view.html.match(/data-recovered-form/g)||[]).length;unassigned+=c.client_number===null;
  for(const r of c.records){
   for(const f of r.e11_source_fields||[])for(const q of f.fields){assert.equal(typeof q.exact_cents,'string');assert.ok(view.html.includes(api.cents(q.exact_cents)));instances++;}
   if(r.source.record_type>=30)for(const f of r.typed.forms||[])for(const q of f.fields){assert.equal(q.keyword_label,null);assert.equal(q.field_namespace,'calculation_form_field_Q_z');}
  }
  if(c.records.some(r=>(r.e11_source_fields||[]).length))assert.ok(view.html.includes('Absent from this source form'));
 }
 assert.equal(fields,26907);assert.equal(forms,1471);assert.equal(instances,113);assert.equal(unassigned,42);
 console.log(JSON.stringify({recovered_contexts:80,member_records:286,forms,fields,exact_cent_requested_line_instances:instances,current_duplicate_profiles_selected:0}));
});
test('native-only source years remain distinct from preserved PDFs and native history',()=>{
 const client={client_number:'100',name:'Synthetic',historical_years:[2014,2024],native_historical_years:[2024],preserved_pdf_figure_years:[],source_archive_years:[],pdf_only_years:[],recovered_native_source_years:[2014],pending_years:[]};
 const page={source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:[2014,2024],clients:[client]};api.validatePage(page,0);
 for(const mutate of [p=>p.clients[0].recovered_native_source_years=[2014,2014],p=>p.clients[0].recovered_native_source_years=[],p=>p.clients[0].pdf_only_years=[2014],p=>p.clients[0].pending_years=[2014]]){const bad=structuredClone(page);mutate(bad);assert.throws(()=>api.validatePage(bad,0));}
});
