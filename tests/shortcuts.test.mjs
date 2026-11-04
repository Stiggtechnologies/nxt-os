import test from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';
const read=p=>fs.readFileSync(new URL('../'+p,import.meta.url),'utf8');
const code=read('public/nxt/shortcuts.js');
function load(pathname='/apps/ledger/'){
  const appended=[];let listener=null;
  const body={appendChild:el=>appended.push(el)};
  const doc={addEventListener:(t,f)=>{if(t==='keydown')listener=f;},getElementById:id=>appended.find(e=>e.id===id&&!e.removed)||null,querySelectorAll:()=>[],querySelector:()=>null,body,
    createElement:()=>{const el={attrs:{},style:{},setAttribute(k,v){this.attrs[k]=v;},addEventListener(){},querySelector:()=>null,remove(){this.removed=true;}};return el;}};
  const loc={pathname,hash:''};const box={document:doc,location:loc};box.globalThis=box;
  vm.runInNewContext(code,box);
  const key=(k,o={})=>{let p=false;listener({key:k,ctrlKey:!!o.ctrl,altKey:!!o.alt,shiftKey:!!o.shift,metaKey:false,target:o.target||{tagName:'BODY'},preventDefault(){p=true;}});return p;};
  return {S:box.NXTShortcuts,loc,key,appended,doc};
}
test('ledger gets the QBO Ctrl+Alt shortcuts and they route to native views',()=>{
  const t=load();
  assert.deepEqual([...t.S.bindings().map(b=>t.S._combo(b))],['Ctrl+Alt+I','Ctrl+Alt+W','Ctrl+Alt+E','Ctrl+Alt+X','Ctrl+Alt+R','Ctrl+Alt+A','Ctrl+Alt+L','Ctrl+Alt+F']);
  assert.equal(t.key('i',{ctrl:true,alt:true}),true);assert.equal(t.loc.hash,'invoices/new');
  t.key('r',{ctrl:true,alt:true});assert.equal(t.loc.hash,'registers');
  t.key('a',{ctrl:true,alt:true});assert.equal(t.loc.hash,'accounts');
  t.key('x',{ctrl:true,alt:true});assert.equal(t.loc.hash,'native-create/expense');
  assert.equal(t.key('e',{ctrl:true,alt:true}),false,'estimate has no native view yet, so it does nothing');
  assert.equal(t.key('i'),false,'bare letters are not shortcuts');
});
test('shortcuts do not fire while typing; other apps do not get ledger keys',()=>{
  const t=load();t.key('i',{ctrl:true,alt:true,target:{tagName:'INPUT'}});assert.equal(t.loc.hash,'');
  assert.equal(load('/apps/tax/').S.bindings().length,0);
});
test('? opens the help overlay listing every shortcut and Esc closes it',()=>{
  const t=load();assert.equal(t.key('?'),true);
  const help=t.appended.find(e=>e.id==='nxt-kb-help');assert.ok(help);assert.equal(help.attrs.role,'dialog');
  assert.match(help.innerHTML,/Ctrl\+Alt\+I/);assert.match(help.innerHTML,/New estimate \(not available here yet\)/);
  assert.equal(t.key('Escape'),true);assert.equal(help.removed,true);
});
test('apps can register their own keys, e.g. tax F-keys',()=>{
  const t=load('/apps/tax/');let ran=0;const off=t.S.register({key:'F5',label:'Recalculate',run:()=>ran++});
  t.key('F5');assert.equal(ran,1);off();t.key('F5');assert.equal(ran,1);
});
test('every app gets shortcuts through the shared company module, not public/index.html',()=>{
  assert.match(read('public/nxt/company-switcher.js'),/\/nxt\/shortcuts\.js/);
});
