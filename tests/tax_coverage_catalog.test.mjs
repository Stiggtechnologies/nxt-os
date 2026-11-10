import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const read=p=>fs.readFileSync(new URL('../'+p,import.meta.url),'utf8');
const esc=v=>String(v??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
function fixture(){
 const programs=['T1','T2','T3','T5013'].map(id=>({id,label:id,entry_schema_id:`nxt-tax-entry/${id}/2025/v1`,entry_schema_year:2025,years:[2025],jurisdictions:id==='T1'?['AB','ON']:id==='T2'?['AB']:[id==='T3'?'CA-FED':'CA'],default_jurisdiction:id==='T1'||id==='T2'?'AB':id==='T3'?'CA-FED':'CA',sections:[],defaults:{scope_confirmed:false,synthetic_entry_value:7}}));
 const scopes=[];for(const p of programs)for(const y of [2025,2026])for(const j of ['AB','ON','QC','CA','CA-FED']){const available=p.id==='T1'?(j==='AB'||j==='ON'||(y===2026&&j==='QC')):p.jurisdictions.includes(j),ready=available&&y===2025;scopes.push({program:p.id,tax_year:y,jurisdiction:j,engine_scope_available:available,browser_entry_available:ready,entry_state:ready?'verified_baseline_schema':available?'year_specific_schema_pending':'unsupported_scope',entry_schema_id:ready?p.entry_schema_id:null,entry_schema_year:ready?2025:null,entry_scope:ready?`${j} 2025 exact entry scope`:null});}
 return {programs,coverage:{schema:'nxt-tax-coverage/v1',version:'2026-11-09.1',years:[2025,2026],scopes,statement:'Limited calculation paths; every scenario needs validation.',browser_entry_statement:'2026 availability is read-only; no 2025 defaults are reused.'}};
}
function workspace(){
 const nodes=new Map(),listeners=new Map(),calls=[];let modalState;
 function node(){return {innerHTML:'',textContent:'',disabled:false,hidden:false,dataset:{},classList:{add(){},remove(){},toggle(){}},querySelectorAll(){return[]},querySelector(s){return get(s)},addEventListener(){},closest(){return {addEventListener(){}}}}}function get(s){if(!nodes.has(s))nodes.set(s,node());return nodes.get(s)}
 const document={querySelector:get,querySelectorAll:()=>[],addEventListener:(type,fn)=>listeners.set(type,fn)};const window={addEventListener(){}};
 const NXT={api:async(path,opts)=>{calls.push({path,opts});return {id:'TEST-created',...opts.body};},esc,money:v=>String(v),toCents:()=>0,toast(){},today(){},modal:(title,init)=>new Promise(resolve=>{
  const controls=new Map();function control(name,value){let html='';const c={value, get innerHTML(){return html},set innerHTML(v){html=v;const opts=[...v.matchAll(/<option value="([^"]*)"([^>]*)>/g)];this.value=(opts.find(x=>x[2].includes('selected'))||opts[0])?.[1]||'';}};controls.set(name,c);return c;}
  for(const [name,value]of Object.entries({program:'T1',tax_year:'2025',jurisdiction:'AB',name:'Synthetic creation',client_ref:'',period_start:'',period_end:''}))control(name,value);
  const form=node();form.reportValidity=()=>true;form.querySelector=s=>{if(s==='[data-create-program]')return controls.get('program');const n=s.match(/^\[name=([^\]]+)\]$/);return n?controls.get(n[1]):get(s)};form.controls=controls;
  const el=node();el.querySelector=s=>s==='form'?form:get(s);el.parentElement={addEventListener(){}};init(el,resolve);modalState={title,form,el,resolve,controls};
 })};
 class FD{constructor(form){this.form=form}get(k){return this.form.controls.get(k)?.value??''}}
 const ctx=vm.createContext({document,window,NXT,FormData:FD,structuredClone,URL,location:{hash:''},console,Date,Promise,setTimeout(){},clearTimeout(){}});
 let source=read('public/apps/tax/tax.js').replace('  window.addEventListener("hashchange", navigate);','globalThis.coverageTest={state,coverageRows,scopeRow,entryRows,entryYears,entryJurisdictions,creationInput,entryDescription,coverageHtml,newReturn,refreshCreationScope,browserEntryAvailable,canEdit,renderWorkspace,calculate,saveReturn,renderKeyword};\n  window.addEventListener("hashchange", navigate);').replace('  boot().catch(','  Promise.resolve().catch(');vm.runInContext(source,ctx);ctx.coverageTest.state.catalog=fixture();return {api:ctx.coverageTest,nodes,calls,listeners,modal:()=>modalState};
}
test('creation uses exact verified year/jurisdiction/schema rows and never copies defaults to 2026',()=>{
 const {api}=workspace(),p=api.state.catalog.programs[0],original=JSON.stringify(p.defaults);assert.deepEqual([...api.entryYears(p)],[2025]);assert.deepEqual([...api.entryJurisdictions(p,2025)],['AB','ON']);assert.equal(api.creationInput(p,2025,'ON').synthetic_entry_value,7);
 for(const [year,j]of [[2025,'QC'],[2026,'AB'],[2026,'QC'],[2014,'AB']])assert.throws(()=>api.creationInput(p,year,j),/An entry form is not available/);
 const cloned=api.creationInput(p,2025,'AB');cloned.synthetic_entry_value=99;assert.equal(JSON.stringify(p.defaults),original);api.state.catalog.coverage.version='unknown';assert.throws(()=>api.creationInput(p,2025,'AB'));assert.equal(api.coverageRows().length,0);
});
test('actual creation dialog changes program and scope, rejects modified 2026 submission before API, then creates one correct 2025 draft',async()=>{
 const w=workspace();w.api.state.year='2026';const pending=w.api.newReturn();await Promise.resolve();const m=w.modal();assert.ok(m);assert.match(m.el.innerHTML,/Calculation and browser-entry coverage/);assert.match(m.el.innerHTML,/Read-only availability/);assert.doesNotMatch(m.el.innerHTML,/<option value="2026"/);assert.equal(m.controls.get('tax_year').value,'2025');
 m.controls.get('jurisdiction').value='ON';await w.listeners.get('change')({target:{matches:s=>s==='[data-create-year],[data-create-jurisdiction]',closest:()=>m.form,name:'jurisdiction'}});assert.equal(w.nodes.get('[data-program-description]').textContent,'ON 2025 exact entry scope');
 m.controls.get('tax_year').value='2026';await m.form.onsubmit({preventDefault(){}});assert.equal(w.calls.length,0);assert.match(w.nodes.get('.form-error').textContent,/An entry form is not available/);
 m.controls.get('program').value='T999';await m.form.onsubmit({preventDefault(){}});assert.equal(w.calls.length,0);assert.match(w.nodes.get('.form-error').textContent,/Choose a program/);
 m.controls.get('tax_year').value='2025';m.controls.get('program').value='T2';await w.listeners.get('change')({target:{matches:s=>s==='[data-create-program]',closest:()=>m.form,name:'program'}});assert.equal(m.controls.get('jurisdiction').value,'AB');assert.match(w.nodes.get('[data-program-description]').textContent,/AB 2025/);
 await m.form.onsubmit({preventDefault(){}});await pending;assert.equal(w.calls.length,1);assert.equal(w.calls[0].path,'/api/tax/returns');assert.equal(w.calls[0].opts.body.program,'T2');assert.equal(w.calls[0].opts.body.tax_year,2025);assert.equal(w.calls[0].opts.body.jurisdiction,'AB');assert.equal(w.calls[0].opts.body.inputs.scope_confirmed,false);assert.equal(w.calls[0].opts.body.client_ref,'');
});
test('coverage renders limited 2026 availability separately and never claims complete returns or certification',()=>{
 const {api}=workspace(),html=api.coverageHtml();assert.match(html,/2026/);assert.match(html,/QC/);assert.match(html,/Read-only availability/);assert.match(html,/2025 workpaper entry available/);api.state.catalog.coverage.scopes.push({...api.state.catalog.coverage.scopes[0]});assert.throws(()=>api.creationInput(api.state.catalog.programs[0],2025,'AB'));
});
test('existing 2026 workpaper is read-only without substituting 2025 fields or requesting a calculation',async()=>{
 const w=workspace(),r={id:'TEST-existing',program:'T1',tax_year:2026,jurisdiction:'QC',current_scope_supported:true,name:'Synthetic API draft',status:'draft',revision:1,input_revision:1,inputs:{qpip_premium_cents:123},notes:'',calculations:[],reviews:[]};Object.assign(w.api.state,{record:r,draft:structuredClone(r),section:'income'});const before=JSON.stringify(w.api.state.draft);w.api.renderWorkspace();assert.equal(w.api.canEdit(r),false);assert.match(w.nodes.get('#main').innerHTML,/Year-specific browser editor pending/);assert.doesNotMatch(w.nodes.get('#main').innerHTML,/Guided interview/);assert.match(w.nodes.get('#return-editor').innerHTML,/Saved inputs remain unchanged/);assert.doesNotMatch(w.nodes.get('#return-editor').innerHTML,/data-keyword=/);assert.equal(JSON.stringify(w.api.state.draft),before);await assert.rejects(w.api.calculate(),/no calculation was requested/);await assert.rejects(w.api.saveReturn(),/input editing remains disabled/);assert.equal(w.calls.length,0);
});
test('2025 scope-confirmation help follows the selected jurisdiction rather than the Alberta template',()=>{const w=workspace();w.api.state.record={program:'T1',tax_year:2025,jurisdiction:'ON'};w.api.state.draft={inputs:{scope_confirmed:false}};w.api.state.errors={};const html=w.api.renderKeyword({key:'scope_confirmed',kind:'boolean',label:'Scope',help:'Wrong Alberta-only text'});assert.match(html,/ON 2025 exact entry scope/);assert.doesNotMatch(html,/Wrong Alberta-only text/);});

function guidedWorkspace(record){
 let stored=structuredClone(record),catalog=fixture();const calls=[],nodes=new Map();
 function node(){return {innerHTML:'',textContent:'',value:'',checked:false,handlers:new Map(),classList:{remove(){},add(){}},addEventListener(type,fn){this.handlers.set(type,fn)},querySelector(s){return get(s)},querySelectorAll(){return[]},closest(){return {classList:{add(){}}}}}}function get(s){if(!nodes.has(s))nodes.set(s,node());return nodes.get(s)}
 const form=get('#f');form.elements={name:{value:record.name},sin:get('sin'),full_year_resident:{checked:true},scope_confirmed:{checked:true},born_1960_or_earlier:{checked:false}};
 const main=node();const start=get('#tt-start');start.controls={prov:'AB',name:'Synthetic new interview',who:'me',spouse:''};
 const api=async(path,options={})=>{
  calls.push({path,options:structuredClone(options)});
  if(path==='/api/tax/catalog')return structuredClone(catalog);
  if(path==='/api/tax/returns'&&options.body){stored={id:'TEST-guided-created',...structuredClone(options.body),revision:1,input_revision:1};return structuredClone(stored);}
  if(path===`/api/tax/returns/${encodeURIComponent(stored.id)}`){if(options.method==='PUT'){stored={...stored,...structuredClone(options.body),revision:stored.revision+1,input_revision:stored.input_revision+1};}return structuredClone(stored);}
  if(path===`/api/tax/returns/${encodeURIComponent(stored.id)}/calculate`){stored.calculation={result:{supported:true,diagnostics:[],totals:{}}};return structuredClone(stored);}
  throw Error('Unexpected synthetic request '+path);
 };
 const ctx=vm.createContext({console,structuredClone,URLSearchParams,location:{hash:''},CSS:{escape:s=>s},sessionStorage:{getItem:()=>null,setItem(){}},localStorage:{getItem:()=>null,setItem(){}},setTimeout(){},FormData:class{constructor(f){this.f=f}get(k){return this.f.controls[k]}}});ctx.window=ctx;ctx.NXT={api,esc,money:v=>String(v)};
 vm.runInContext(read('public/apps/tax/tt-core.js'),ctx);
 vm.runInContext(read('public/apps/tax/tt-ui.js').replace('  window.NXTTurbo = {','  globalThis.guidedGuardTest={saveAndCalc,guidedEntryAvailable};\n  window.NXTTurbo = {'),ctx);
 return {ctx,main,nodes,calls,stored:()=>stored,catalog:()=>catalog,setCatalog:v=>catalog=v,form,start};
}
function guidedRecord(year,jurisdiction,available=true){return {id:'TEST-guided',program:'T1',tax_year:year,jurisdiction,current_scope_supported:available,name:'Synthetic saved interview',revision:1,input_revision:1,status:'draft',inputs:{employment_income_cents:12345,qpip_premium_cents:789},notes:'Synthetic only'};}
test('direct guided bookmarks for 2026 and unsupported 2025 QC show read-only notice before every interview/review screen',async()=>{
 for(const [year,jurisdiction,available]of [[2026,'AB',true],[2026,'QC',true],[2025,'QC',false]])for(const screen of ['profile','situations','slips','deductions','review','pay']){
  const original=guidedRecord(year,jurisdiction,available),w=guidedWorkspace(original),before=JSON.stringify(w.stored());await w.ctx.NXTTurbo.route('tt-t1',original.id,'screen='+screen,w.main);
  assert.match(w.main.innerHTML,/Guided entry unavailable/);assert.match(w.main.innerHTML,/Saved inputs remain unchanged/);assert.doesNotMatch(w.main.innerHTML,/<form|Ready for preparer review|Continue to pay/);assert.equal(w.form.handlers.size,0);assert.equal(JSON.stringify(w.stored()),before);assert.ok(w.calls.every(c=>!c.options.method&&!c.options.body));assert.ok(!w.calls.some(c=>/calculate|aggregate|stripe/.test(c.path)));
 }
});
test('guided save rechecks live catalog before PUT even for an already-open supported profile handler',async()=>{
 const r=guidedRecord(2025,'AB'),w=guidedWorkspace(r);await w.ctx.NXTTurbo.route('tt-t1',r.id,'screen=profile',w.main);assert.match(w.main.innerHTML,/About you/);assert.ok(w.form.handlers.has('submit'));
 const before=JSON.stringify(w.stored());w.catalog().coverage.version='unknown';await w.form.handlers.get('submit')({preventDefault(){}});assert.match(w.nodes.get('.form-error').textContent,/guided entry form is unavailable/);assert.equal(JSON.stringify(w.stored()),before);assert.ok(!w.calls.some(c=>c.options.method==='PUT'||/calculate/.test(c.path)));
 const v=guidedWorkspace(guidedRecord(2026,'QC'));await assert.rejects(v.ctx.guidedGuardTest.saveAndCalc(v.stored(),{employment_income_cents:999}),/guided entry form is unavailable/);assert.ok(!v.calls.some(c=>c.options.method==='PUT'||/calculate/.test(c.path)));
});
test('supported 2025 guided profile still saves and calculates with its exact original year/jurisdiction',async()=>{
 const r=guidedRecord(2025,'ON'),w=guidedWorkspace(r);await w.ctx.NXTTurbo.route('tt-t1',r.id,'screen=profile',w.main);await w.form.handlers.get('submit')({preventDefault(){}});
 const put=w.calls.filter(c=>c.options.method==='PUT');assert.equal(put.length,1);assert.equal(put[0].options.body.tax_year,2025);assert.equal(put[0].options.body.jurisdiction,'ON');assert.equal(put[0].options.body.inputs.employment_income_cents,12345);assert.equal(w.calls.filter(c=>c.path.endsWith('/calculate')).length,1);assert.ok(w.calls.filter(c=>c.path==='/api/tax/catalog').length>=2);assert.match(w.ctx.location.hash,/screen=situations/);
});
test('guided Start filters the same catalog and rejects modified unsupported province before POST',async()=>{
 const w=guidedWorkspace(guidedRecord(2025,'AB'));await w.ctx.NXTTurbo.route('tt-start',null,'',w.main);assert.match(w.main.innerHTML,/Who is this return for/);assert.doesNotMatch(w.main.innerHTML,/<option value="QC"/);w.start.controls.prov='QC';await w.start.handlers.get('submit')({preventDefault(){}});assert.match(w.nodes.get('#tt-err').textContent,/guided entry form is unavailable/);assert.equal(w.calls.filter(c=>c.path==='/api/tax/returns').length,0);
 w.start.controls.prov='AB';await w.start.handlers.get('submit')({preventDefault(){}});const posts=w.calls.filter(c=>c.path==='/api/tax/returns');assert.equal(posts.length,1);assert.equal(posts[0].options.body.tax_year,2025);assert.equal(posts[0].options.body.jurisdiction,'AB');
 const closed=guidedWorkspace(guidedRecord(2025,'AB'));closed.catalog().coverage.version='unknown';await closed.ctx.NXTTurbo.route('tt-start',null,'',closed.main);assert.match(closed.main.innerHTML,/Guided entry unavailable/);assert.equal(closed.calls.filter(c=>c.options.body).length,0);
});
