import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const read=p=>fs.readFileSync(new URL('../'+p,import.meta.url),'utf8');
const esc=v=>String(v??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
function workspace(){
 const nodes=new Map();const node=()=>({innerHTML:'',textContent:'',disabled:false,classList:{add(){},toggle(){}},querySelectorAll(){return[]},querySelector(s){return get(s)}});const get=s=>{if(!nodes.has(s))nodes.set(s,node());return nodes.get(s)};
 const document={querySelector:get,querySelectorAll:()=>[],addEventListener(){}};const window={addEventListener(){}};const NXT={api:()=>{throw Error('no network')},esc,money:v=>String(v),toCents:()=>0,toast(){},modal(){},today(){}};
 const ctx=vm.createContext({document,window,NXT,structuredClone,URL,location:{hash:''},console,Date,Promise,setTimeout(){},clearTimeout(){}});
 const source=read('public/apps/tax/tax.js').replace('  window.addEventListener("hashchange", navigate);','globalThis.scopeTest={state,freshCalculation,renderWorkspace,renderCalculation};\n  window.addEventListener("hashchange", navigate);').replace('  boot().catch(','  Promise.resolve().catch(');vm.runInContext(source,ctx);return {api:ctx.scopeTest,nodes};
}
function legacy(supported){return {id:'TEST-only',program:'T1',tax_year:2025,jurisdiction:'QC',name:'Synthetic scope',client_ref:'TEST-only',status:'calculated',current_scope_supported:supported,revision:2,input_revision:1,inputs:{},calculation:{input_revision:1,result:{supported:true,diagnostics:[],totals:{balance:-100},lines:[]}},calculations:[],reviews:[],notes:''}}
test('actual workpaper render qualifies saved invalid-scope snapshots and removes review actions',()=>{
 const {api,nodes}=workspace();const r=legacy(false);api.state.catalog={programs:[{id:'T1',label:'T1',years:[2025],jurisdictions:['AB'],sections:[]}]};Object.assign(api.state,{record:r,draft:structuredClone(r),dirty:false,section:'calculation'});api.renderWorkspace();
 assert.equal(api.freshCalculation(),false);assert.match(nodes.get('#main').innerHTML,/Unsupported current calculation scope/);assert.doesNotMatch(nodes.get('#main').innerHTML,/data-action="(?:review-return|approve-return)"/);assert.doesNotMatch(nodes.get('#result-panel').innerHTML,/Calculated · supported scope/);assert.doesNotMatch(nodes.get('#return-editor').innerHTML,/Current input revision/);assert.equal(r.calculation.result.supported,true,'snapshot remains unchanged');
 api.state.record=legacy(true);api.state.draft=structuredClone(api.state.record);api.renderWorkspace();assert.equal(api.freshCalculation(),true);assert.match(nodes.get('#main').innerHTML,/data-action="review-return"/);
});
test('guided review and refund meter block historical unsupported scope without changing saved result',()=>{
 const ctx=vm.createContext({window:{NXTTurboCore:{},NXT:{esc,money:()=>''}},console});const source=read('public/apps/tax/tt-ui.js').replace('  const SCREENS =','  globalThis.scopeCalcOf=calcOf;\n  const SCREENS =');vm.runInContext(source,ctx);const r=legacy(false),old=JSON.stringify(r);const c=ctx.scopeCalcOf(r);assert.equal(c.supported,false);assert.equal(Object.keys(c.totals).length,0);assert.ok(c.diagnostics.some(d=>d.code==='unsupported_current_scope'&&d.blocks_review));assert.equal(JSON.stringify(r),old);const good=legacy(true);assert.equal(ctx.scopeCalcOf(good),good.calculation.result);
});
