import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

// Run the actual application router/event handlers with synthetic feature hosts.
// No production identity, API or client record is used.
const source=fs.readFileSync(new URL('../public/apps/tax/tax.js',import.meta.url),'utf8');
function app(){
  const nodes=new Map(),events=new Map(),mounts=[],calls=[];
  const get=s=>{if(s==='.modal-back')return null;if(!nodes.has(s))nodes.set(s,{innerHTML:'',textContent:'',dataset:{},isConnected:true,classList:{add(){},remove(){},toggle(){}},setAttribute(){},removeAttribute(){},querySelector:get,querySelectorAll:()=>[],focus(){}});return nodes.get(s);};
  const document={querySelector:get,querySelectorAll:()=>[],addEventListener:(name,fn)=>events.set(name,fn)};
  const location={hash:'#reports',origin:'https://synthetic.invalid'};
  const api=(...args)=>{calls.push(args);return Promise.resolve({});};
  const mounted=(kind,options)=>{const h={kind,options,disposed:false,ready:Promise.resolve(),dispose(){this.disposed=true;}};mounts.push(h);return h;};
  const window={addEventListener(){},NXTCompany:{get:()=>'',onChange(){}},NXTTaxReports:{mount:(_el,o)=>mounted('reports',o)},NXTBatch:{mount:(_el,o)=>mounted('batches',o)},NXTFamilyWorkflow:{route:async(_id,_el,o)=>{mounted('family',o);},discard(){}}};window.parent=window;
  const NXT={api,esc:v=>String(v??''),money:String,toCents:()=>0,toast(){},modal(){throw Error('unexpected modal');},today(){return '2026-11-10';}};
  const ctx=vm.createContext({document,window,location,NXT,NXTTaxReports:window.NXTTaxReports,NXTBatch:window.NXTBatch,NXTFamilyWorkflow:window.NXTFamilyWorkflow,structuredClone,URL,URLSearchParams,console,Date,Promise,setTimeout(){},clearTimeout(){}});
  const instrumented=source.replace('  window.addEventListener("hashchange", navigate);','  globalThis.routerTest={state,navigate};\n  window.addEventListener("hashchange", navigate);').replace('  boot().catch(','  Promise.resolve().catch(');
  vm.runInContext(instrumented,ctx);assert.ok(ctx.routerTest,'router instrumentation must attach');ctx.routerTest.state.catalog={programs:[]};
  return {...ctx.routerTest,nodes,events,mounts,calls,location};
}
for(const route of ['reports','batches','family-workspaces','family-workspace/synthetic-family']){
  test(`${route} clears the previous return before global save/calculation shortcuts`,async()=>{
    const a=app();Object.assign(a.state,{record:{id:'prior-synthetic-return'},draft:{inputs:{old:1}},dirty:false});a.location.hash='#'+route;await a.navigate();
    assert.equal(a.state.record,null);assert.equal(a.state.draft,null);assert.equal(a.state.dirty,false);assert.equal(a.mounts.length,1);assert.equal(a.mounts[0].options.isCurrent(),true);
    let prevented=0;for(const key of ['s','Enter'])a.events.get('keydown')({ctrlKey:true,key,preventDefault(){prevented++;}});
    assert.equal(a.calls.length,0,'a standalone workspace cannot save or calculate the prior return');assert.equal(prevented,0);
  });
}
test('moving between feature hosts disposes reports/batches and invalidates every prior route',async()=>{
  const a=app();await a.navigate();const report=a.mounts.at(-1);a.location.hash='#batches';await a.navigate();const batch=a.mounts.at(-1);
  assert.equal(report.disposed,true);assert.equal(report.options.isCurrent(),false);assert.equal(batch.options.isCurrent(),true);
  a.location.hash='#family-workspaces';await a.navigate();assert.equal(batch.disposed,true);assert.equal(batch.options.isCurrent(),false);
  const family=a.mounts.at(-1);a.location.hash='#reports';await a.navigate();assert.equal(family.options.isCurrent(),false);assert.equal(a.mounts.at(-1).options.isCurrent(),true);
});
