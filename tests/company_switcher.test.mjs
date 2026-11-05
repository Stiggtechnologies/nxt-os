import test from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';
const read=p=>fs.readFileSync(new URL('../'+p,import.meta.url),'utf8');
const code=read('public/nxt/company-switcher.js');
function load({search='',top=true}={}){
  const store=new Map(),posted=[],handlers={};
  const doc={readyState:'complete',addEventListener:(t,f)=>handlers[t]=f,querySelectorAll:()=>[],createElement:()=>({}),head:{appendChild(){}},documentElement:{}};
  const win={};const box={localStorage:{getItem:k=>store.get(k)??null,setItem:(k,v)=>store.set(k,v),removeItem:k=>store.delete(k)},location:{search,href:'https://x.test/apps/ledger/'+search,pathname:'/apps/ledger/',hash:''},history:{state:null,replaceState(s,t,u){box.location.search=new URL(u,'https://x.test').search;box.location.href='https://x.test'+u;}},document:doc,MutationObserver:class{observe(){}},URL,URLSearchParams};
  box.window=win;win.parent=top?win:{postMessage:(m)=>posted.push(m)};box.globalThis=box;
  vm.runInNewContext(code,box);return {C:box.NXTCompany,store,posted,handlers,box};
}
test('active company comes from ?company=, then localStorage',()=>{
  assert.equal(load({search:'?company=co-a'}).C.get(),'co-a');
  assert.equal(load({search:'?company=co-b'}).C.get(),'co-b');
  const t=load();t.store.set('nxt_active_client','co-c');assert.equal(t.C.get(),'co-c');
});
test('in the shell set persists, puts ?company= in the URL and notifies the header',()=>{
  const t=load();const seen=[];t.C.onChange(id=>seen.push(id));t.C.set('co-a');
  assert.equal(t.store.get('nxt_active_client'),'co-a');assert.match(t.box.location.search,/company=co-a/);assert.deepEqual(seen,['co-a']);
  t.C.set('');assert.equal(t.store.has('nxt_active_client'),false);
});
test('inside an app set tells the shell instead of navigating on its own',()=>{
  const t=load({top:false});t.C.openJourney('co-a');assert.deepEqual(JSON.parse(JSON.stringify(t.posted)),[{nxt:'company',id:'co-a',open:true}]);
});
test('list uses the firm roster and falls back to assigned clients for scoped staff',async()=>{
  const {C}=load();
  const all=await C.list(async p=>({clients:[{client_ref:'co-a',name:'Synthetic A',aliases:['qbo-a','bad id']}]}));assert.deepEqual(JSON.parse(JSON.stringify(all)),[{id:'co-a',name:'Synthetic A',aliases:['qbo-a']}]);
  assert.deepEqual([...C.aliasesOf('QBO-A',all)].sort(),['co-a','qbo-a']);assert.equal(C.canonical('qbo-a',all),'co-a');assert.equal(C.nameOf('qbo-a',all),'Synthetic A');
  assert.equal(C.validId('a'.repeat(80)),true);assert.equal(C.validId('a'.repeat(81)),false);assert.equal(C.validId('-x'),false);assert.equal(C.validId('a:b'),false);
  const scoped=await C.list(async p=>{if(p==='/api/clients')throw Object.assign(Error('down'),{status:503});return {companies:[{company_id:'co-b',company_name:''}]};});
  assert.deepEqual(scoped.map(r=>r.id),['co-b']);
  await assert.rejects(C.list(async()=>{throw Object.assign(Error('x'),{status:401});}));
});
test('row clicks open the journey; inner buttons and Enter/Space follow the rules',()=>{
  const t=load({top:false});
  const row={getAttribute:()=> 'co-r',contains:()=>true};
  const btn={closest:s=>s.includes('button')?btn:(s==='[data-client-row]'?row:null)};
  const cell={closest:s=>s==='[data-client-row]'?row:null};
  t.handlers.click({target:btn});assert.equal(t.posted.length,0);
  t.handlers.click({target:cell});assert.equal(t.posted.at(-1).id,'co-r');
  let prevented=false;row.closest=s=>s==='[data-client-row]'?row:null;t.handlers.keydown({key:' ',target:row,preventDefault(){prevented=true;}});assert.equal(prevented,true);assert.equal(t.posted.length,2);
  t.handlers.keydown({key:'a',target:row,preventDefault(){}});assert.equal(t.posted.length,2);
});
test('every app and the shell load the one shared module; lists are marked as client rows',()=>{
  for(const app of ['ledger','firm','tax','engagements','signatures','migrations','referrals','guardvoice','nixon'])assert.match(read(`public/apps/${app}/index.html`),/\/nxt\/company-switcher\.js/);
  const shell=read('public/index.html');assert.match(shell,/id="companySwitch"/);assert.match(shell,/mountSwitcher/);assert.doesNotMatch(shell,/window\.companySwitch\b/,'window.companySwitch is the #companySwitch element (named access)');assert.match(shell,/"\?company="/);
  assert.match(read('public/apps/firm/firm.js'),/data-client-row="\$\{esc\(c\.client_ref\)\}"/);assert.match(read('public/apps/firm/firm.js'),/client:c=>c\.company_id/);
  assert.doesNotMatch(read('public/apps/firm/firm.js'),/native company linking planned/);
  assert.match(read('public/apps/migrations/migrations.js'),/data-client-row="\$\{esc\(c\.company_id\)\}"/);
  assert.match(read('public/apps/nixon/nixon.js'),/data-client-row=/);assert.match(read('public/apps/nixon/nixon.js'),/#client/);
});
test('roster is cached in sessionStorage and the switcher gives up after 10 s, not 45 s',async()=>{
  assert.match(code,/timeoutMs = 10000/);assert.doesNotMatch(code,/45000/);
  const t=load();const ss=new Map();t.box.sessionStorage={getItem:k=>ss.get(k)??null,setItem:(k,v)=>ss.set(k,v),removeItem:k=>ss.delete(k)};
  assert.equal(t.C.cachedRoster(),null);
  t.C.saveRoster([{id:'example-co-001',name:'Example Co 001'}],1000);
  assert.deepEqual(JSON.parse(JSON.stringify(t.C.cachedRoster(2000))),[{id:'example-co-001',name:'Example Co 001'}]);
  assert.equal(t.C.cachedRoster(1000+6*60*1000),null,'stale after 5 minutes');
});
test('type-ahead filters by name or id, prefix matches first',()=>{
  const {C}=load();const rows=[{id:'b-2',name:'Northern Example'},{id:'a-1',name:'Example Co 001'},{id:'c-3',name:'Other'}];
  assert.deepEqual(C.filter(rows,'exa').map(r=>r.id),['a-1','b-2']);
  assert.deepEqual(C.filter(rows,'C-3').map(r=>r.id),['c-3']);
  assert.equal(C.filter(rows,'').length,3);
});
