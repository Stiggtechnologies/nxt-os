import test from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';
const source=fs.readFileSync(new URL('../public/apps/tax/family-workflow.js',import.meta.url),'utf8');
function harness(){
 let company='',changed,mutations=[];const nodes=new Map(),dirty=[];
 const get=s=>{if(!nodes.has(s))nodes.set(s,{innerHTML:'',textContent:'',value:'',checked:false,disabled:false,isConnected:true,dataset:{},handlers:{},querySelector:get,querySelectorAll(){return[]},addEventListener(t,f){this.handlers[t]=f}});return nodes.get(s)};
 const side=(id)=>({id,name:'Synthetic '+id,tax_year:2025,program:'T1',jurisdiction:'AB',client_ref:'',revision:1,input_revision:1,status:'draft',inputs:{scope_confirmed:false,employment_income_cents:0}});
 const record={schema:'nxt-tax-family-workspace/v1',family:{id:'F',label:'Synthetic family',primary_return_id:'A',spouse_return_id:'B'},automatic_plan_apply_available:false,write_permitted:true,family_sha256:'a'.repeat(64),client_ref:'',tax_year:2025,primary:side('A'),spouse:side('B'),paired_entry_available:true,plan_blockers:['Informational only']};
 const fields=[{key:'scope_confirmed',label:'Scope',kind:'boolean'},{key:'employment_income_cents',label:'Employment',kind:'money'}];const catalog={programs:[{id:'T1',entry_schema_id:'T1/v1',entry_schema_year:2025,sections:[{fields}]}],coverage:{schema:'nxt-tax-coverage/v1',version:'2026-11-09.1',scopes:[{program:'T1',tax_year:2025,jurisdiction:'AB',engine_scope_available:true,browser_entry_available:true,entry_state:'verified_baseline_schema',entry_schema_id:'T1/v1',entry_schema_year:2025}]}};
 const list={write_permitted:true,families:[{id:'F',label:'Family',client_ref:''}],candidates:[record.primary,record.spouse]};
 const ctx=vm.createContext({structuredClone,console,location:{hash:''},FormData:class{constructor(f){this.f=f}get(k){return this.f.values?.[k]??''}}});ctx.window=ctx;ctx.NXTCompany={KEY:'synthetic-company',get:()=>company,onChange:f=>changed=f};ctx.addEventListener=()=>{};
 ctx.NXT={esc:v=>String(v??''),toCents:v=>/^\d+(\.\d{1,2})?$/.test(v)?Math.round(Number(v)*100):NaN,api:async(path,opts)=>{if(opts?.method){mutations.push({path,opts:structuredClone(opts)});return structuredClone(record);}return structuredClone(path==='/api/tax/catalog'?catalog:path==='/api/tax/family-workspaces'?list:record);}};
 vm.runInContext(source,ctx);return{api:ctx.NXTFamilyWorkflow,ctx,record,catalog,list,nodes,get,main:get('main'),dirty,mutations,scope:v=>{company=v;changed();},mount:()=>ctx.NXTFamilyWorkflow.route('F',get('main'),{setDirty:v=>dirty.push(v)})};
}
test('typed member selection rejects same person, wrong year or company and binds both revisions',()=>{
 const h=harness(),r=h.list.candidates;assert.equal(h.api.pairSelection(r,'A','B','').expected_spouse_revision,1);assert.throws(()=>h.api.pairSelection(r,'A','A',''));r[1].tax_year=2026;assert.throws(()=>h.api.pairSelection(r,'A','B',''));r[1].tax_year=2025;r[1].client_ref='other';assert.throws(()=>h.api.pairSelection(r,'A','B',''));
});
test('existing company aliases scope the view without rewriting stored client references',()=>{
 const h=harness();h.ctx.NXTCompany.inScope=(ref,scope)=>scope==='canonical'&&ref==='legacy-alias';for(const r of h.list.candidates)r.client_ref='legacy-alias';const p=h.api.pairSelection(h.list.candidates,'A','B','canonical');assert.equal(p.expected_client_ref,'legacy-alias');h.list.candidates[1].client_ref='canonical';assert.throws(()=>h.api.pairSelection(h.list.candidates,'A','B','canonical'));
});
test('actual family UI saves exactly one paired request and preserves false confirmations',async()=>{
 const h=harness();await h.mount();assert.match(h.main.innerHTML,/Side-by-side data entry/);assert.match(h.main.innerHTML,/Automatic proposal application is unavailable/);
 const pf=h.get('[data-pair]');for(const s of ['primary','spouse'])h.get(`[data-side="${s}"][data-key="employment_income_cents"]`).value=s==='primary'?'12.34':'56.78';
 const capturedSubmit=pf.onsubmit;pf.handlers.input({target:h.get('[data-side="primary"][data-key="employment_income_cents"]')});await capturedSubmit({preventDefault(){}});
 assert.equal(h.mutations.length,1);const {path,opts}=h.mutations[0];assert.equal(path,'/api/tax/family-workspaces/F/inputs');assert.equal(opts.method,'PUT');assert.equal(opts.body.primary_inputs.employment_income_cents,1234);assert.equal(opts.body.spouse_inputs.employment_income_cents,5678);assert.equal(opts.body.primary_inputs.scope_confirmed,false);assert.equal(opts.body.expected_client_ref,'');assert.equal(opts.body.expected_family_sha256,'a'.repeat(64));assert.ok(!/calculate|accept|optimize/.test(path));
 await capturedSubmit({preventDefault(){}});assert.equal(h.mutations.length,1,'old mounted callback cannot replay after refresh');
});
test('workspace roundtrip and detached form prevent writes',async()=>{
 for(const mode of ['scope','detached']){const h=harness();await h.mount();const pf=h.get('[data-pair]');if(mode==='scope'){h.scope('other');h.scope('');}else pf.isConnected=false;await pf.onsubmit({preventDefault(){}});assert.equal(h.mutations.length,0);}
});
test('dirty membership blocks paired write and dirty paired entry blocks membership replacement',async()=>{
 const h=harness();await h.mount();const mf=h.get('[data-family-members]'),pf=h.get('[data-pair]');mf.handlers.input();await pf.onsubmit({preventDefault(){}});assert.equal(h.mutations.length,0);assert.match(h.get('[data-pair-error]').textContent,/family member changes/);
 const x=harness();await x.mount();x.get('[data-pair]').handlers.input({target:x.get('changed')});x.get('[data-family-members]').values={label:'New',primary:'A',spouse:'B'};await x.get('[data-family-members]').onsubmit({preventDefault(){}});assert.equal(x.mutations.length,0);assert.match(x.get('[data-members-error]').textContent,/paired input changes/);
});
test('unverified catalog or backend unavailable pair renders no paired Save control',async()=>{
 const h=harness();h.catalog.coverage.version='unknown';await h.mount();assert.doesNotMatch(h.main.innerHTML,/<button[^>]*data-save-pair/);assert.match(h.main.innerHTML,/read-only here/);await h.get('[data-pair]').onsubmit({preventDefault(){}});assert.equal(h.mutations.length,0);
});
test('mismatched family response scope, members or version cannot render an editor',async()=>{
 for(const change of ['version','family','member','company','year']){const h=harness();if(change==='version')h.record.schema='unknown';if(change==='family')h.record.family.id='different';if(change==='member')h.record.family.spouse_return_id='A';if(change==='company')h.record.spouse.client_ref='other';if(change==='year')h.record.spouse.tax_year=2026;await h.mount();assert.match(h.main.innerHTML,/Family workspace unavailable/);assert.doesNotMatch(h.main.innerHTML,/data-save-pair/);assert.equal(h.mutations.length,0);}
});

test('assigned read-only staff can compare but cannot submit either mutation',async()=>{
 const h=harness();h.scope('A');h.list.families[0].client_ref='A';h.record.client_ref='A';for(const r of h.list.candidates)r.client_ref='A';h.list.write_permitted=false;h.record.write_permitted=false;h.record.paired_entry_available=false;await h.mount();assert.match(h.main.innerHTML,/read.only/i);assert.doesNotMatch(h.main.innerHTML,/<button[^>]*data-save-pair/);assert.doesNotMatch(h.main.innerHTML,/<button[^>]*data-save-members/);const mf=h.get('[data-family-members]');mf.values={label:'Blocked',primary:'A',spouse:'B'};await mf.onsubmit({preventDefault(){}});await h.get('[data-pair]').onsubmit({preventDefault(){}});assert.equal(h.mutations.length,0);
});
