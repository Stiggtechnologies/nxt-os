import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const read=p=>fs.readFileSync(new URL('../'+p,import.meta.url),'utf8');
const code=read('public/apps/ledger/company-books-bridge.js');
const esc=v=>String(v??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');
const controls={source_authoritative:true,parallel_test:true,live_cutover_allowed:false};
const roster=rows=>({companies:rows,count:rows.length,financial_imports_inferred:false,inventory_complete:false});
const source=rows=>({snapshots:rows,shadow_only:true,source_authoritative:true,live_cutover_allowed:false});
const book=(id,committed=true)=>({company_id:id,stage:committed?'import_committed':'staged',head:committed?{company_id:id}:null,head_sha256:committed?'a'.repeat(64):null,previews:[{company:{name:'Synthetic '+id}}]});
const page=(rows,total=rows.length,offset=0,more=false)=>({companies:rows,total,offset,has_more:more,controls});
function harness(handler){
  const box={AbortController,setTimeout,clearTimeout,console};vm.runInNewContext(code,box);
  let html='',nodes=new Map();
  const host={isConnected:true,get innerHTML(){return html;},set innerHTML(s){html=s;nodes=new Map();},querySelector(selector){
    const exists=selector[0]==='#'?html.includes('id="'+selector.slice(1)+'"'):html.includes(selector.slice(1,-1));
    if(!exists)return null;if(!nodes.has(selector))nodes.set(selector,{value:'',onclick:null,onchange:null});return nodes.get(selector);
  }};
  const calls=[],opened=[];let respond=handler||((path)=>path.endsWith('/roster')?roster([{company_id:'observed-A',company_name:'Synthetic A'}]):path==='/api/migrations'?source([]):page([]));
  const api=async(path,options)=>{calls.push({path,signal:options.signal});return respond(path,options);};
  return {subject:box.NXTLedgerCompanyBridge,host,calls,opened,setHandler(h){respond=h;},mount(options={}){return box.NXTLedgerCompanyBridge.mount(host,{api,esc,openApp:(app,view)=>opened.push({app,view}),...options});}};
}
test('observed and imported companies remain distinct stable cohorts despite equal labels',async()=>{
 const h=harness(path=>path.endsWith('/roster')?roster([{company_id:'observed-A',company_name:'Same label'},{company_id:'imported-B',company_name:'Same label'}]):path==='/api/migrations'?source([]):page([book('imported-B')]));
 const m=h.mount();await m.ready;assert.match(h.host.innerHTML,/value="observed-A"/);assert.match(h.host.innerHTML,/value="imported-B"/);assert.match(h.host.innerHTML,/Journal imported/);assert.match(h.host.innerHTML,/Observed only/);
 const select=h.host.querySelector('#ledger-company-choice');select.value='imported-B';select.onchange();assert.deepEqual(h.opened,[{app:'migrations',view:'company/imported-B/trial-balance'}]);assert.ok(h.calls.every(c=>!c.path.includes('/api/ledger')&&c.signal));m.destroy();
});
test('observed and report-only choices open scoped receipts rather than global empty books',async()=>{
 for(const reports of [[],[{id:'snapshot-A',cohort_ref:'observed-A',company_name:'Synthetic A'}]]){
  const h=harness(path=>path.endsWith('/roster')?roster([{company_id:'observed-A',company_name:'Synthetic A'}]):path==='/api/migrations'?source(reports):page([]));const m=h.mount();await m.ready;
  assert.match(h.host.innerHTML,reports.length?/Reports saved · no journal import/:/Observed only/);const select=h.host.querySelector('#ledger-company-choice');select.value='observed-A';select.onchange();assert.equal(h.opened[0].view,'company/observed-A');m.destroy();
 }
});
test('unsafe names are escaped and unlisted/tampered IDs never navigate',async()=>{
 const h=harness(path=>path.endsWith('/roster')?roster([{company_id:'safe-A',company_name:'<img src=x onerror="boom">'}]):path==='/api/migrations'?source([]):page([]));const m=h.mount();await m.ready;
 assert.ok(!h.host.innerHTML.includes('<img'));assert.match(h.host.innerHTML,/&#60;img/);const select=h.host.querySelector('#ledger-company-choice');select.value='../other';select.onchange();assert.equal(h.opened.length,0);
 for(const id of ['../x','https://x','x%2fy','x" onclick="x',''])assert.throws(()=>h.subject.companyView(id));assert.throws(()=>h.subject.companyView('safe-A','invoices/new'));m.destroy();
});
test('bounded metadata pagination verifies totals and emits no per-company head requests',async()=>{
 const h=harness();const calls=[];const rows=await h.subject.booksRows(async path=>{calls.push(path);return calls.length===1?page([book('A')],2,0,true):page([book('B')],2,1,false);},()=>true);
 assert.equal(rows.length,2);assert.deepEqual(calls,['/api/company-books?offset=0&limit=50','/api/company-books?offset=1&limit=50']);
 for(const response of [page([],1,0,false),page([],1,0,true),page([book('A'),book('A')],2),page([book('A')],501)])await assert.rejects(()=>h.subject.booksRows(async()=>response,()=>true));
 let n=0;await assert.rejects(()=>h.subject.booksRows(async()=>++n===1?page([book('A')],2,0,true):page([book('B')],3,1,false),()=>true));
});
test('inconsistent committed status and unsafe source/roster references fail closed',async()=>{
 const h=harness();assert.throws(()=>h.subject.rosterRows(roster([{company_id:'../x',company_name:'Synthetic'}])));assert.throws(()=>h.subject.sourceRows(source([{id:'snap',cohort_ref:'../x',company_name:'Synthetic'}])));
 await assert.rejects(()=>h.subject.booksRows(async()=>page([{...book('A'),head:{company_id:'B'}}]),()=>true));await assert.rejects(()=>h.subject.booksRows(async()=>page([{...book('A'),head:null}]),()=>true));
});
test('401 denies the whole directory visibly and does not claim zero source records',async()=>{
 const h=harness(path=>{if(path.endsWith('/roster')){const e=Error('Unauthorized');e.status=401;throw e;}return path==='/api/migrations'?source([]):page([]);});const m=h.mount();await m.ready;
 assert.match(h.host.innerHTML,/Sign in to view company books/);assert.match(h.host.innerHTML,/disabled/);assert.ok(!h.host.innerHTML.includes('Observed only'));assert.equal(h.opened.length,0);m.destroy();
});
test('partial API failures preserve unknown status instead of no-import/empty claims',async()=>{
 const h=harness(path=>{if(path.startsWith('/api/company-books?'))throw Error('Unavailable');return path.endsWith('/roster')?roster([{company_id:'A',company_name:'Synthetic A'}]):source([{id:'snap',cohort_ref:'A',company_name:'Synthetic A'}]);});const m=h.mount();await m.ready;
 assert.match(h.host.innerHTML,/Reports saved · journal status unavailable/);assert.match(h.host.innerHTML,/Some company status is unavailable/);assert.ok(!h.host.innerHTML.includes('no journal import'));m.destroy();
});
test('destroy and superseding reload discard old responses and abort their own requests',async()=>{
 const pending=[];const h=harness((path,{signal})=>new Promise((resolve,reject)=>{pending.push({path,signal,resolve,reject});}));const first=h.mount();const old=h.host.innerHTML;
 first.destroy();for(const p of pending){assert.equal(p.signal.aborted,true);p.resolve(p.path.endsWith('/roster')?roster([]):p.path==='/api/migrations'?source([]):page([]));}await first.ready;assert.equal(h.host.innerHTML,old);
 const h2=harness((path,{signal})=>new Promise((resolve,reject)=>pending.push({path,signal,resolve,reject})));pending.length=0;const second=h2.mount();const oldPending=pending.splice(0);h2.setHandler(path=>path.endsWith('/roster')?roster([{company_id:'new-B',company_name:'Synthetic new'}]):path==='/api/migrations'?source([]):page([]));await second.reload();
 const before=h2.host.innerHTML;const error=Object.assign(Error('old auth'),{status:401});oldPending[0].reject(error);for(const p of oldPending.slice(1))p.resolve(p.path==='/api/migrations'?source([]):page([]));await second.ready;assert.equal(h2.host.innerHTML,before);assert.match(before,/new-B/);assert.ok(h2.calls.slice(3).every(call=>!call.signal.aborted));second.destroy();
});
test('timeout aborts the metadata requests and never admits late data',async()=>{
 const h=harness((path,{signal})=>new Promise((resolve,reject)=>{signal.addEventListener('abort',()=>reject(Object.assign(Error('aborted'),{name:'AbortError'})),{once:true});}));const m=h.mount({timeoutMs:5});await m.ready;assert.match(h.host.innerHTML,/timed out/);assert.ok(h.calls.every(c=>c.signal.aborted));m.destroy();
});
test('directory action and timeout bounds stay on the existing read-only company route',async()=>{
 const h=harness();for(const timeoutMs of [0,-1,Infinity,60001])assert.throws(()=>h.mount({timeoutMs}));assert.equal(h.calls.length,0);
 const m=h.mount();await m.ready;let prevented=false;h.host.querySelector('[data-company-directory]').onclick({preventDefault(){prevented=true;}});assert.equal(prevented,true);assert.deepEqual(h.opened,[{app:'migrations',view:'companies'}]);m.destroy();
});
test('native default, startup hooks and legacy Ledger API routes remain intact',()=>{
 const ledger=read('public/apps/ledger/ledger.js'),index=read('public/apps/ledger/index.html');
 assert.match(index,/id="ledger-company-bridge"/);assert.ok(index.indexOf('company-books-bridge.js')<index.indexOf('src="ledger.js"'));
 assert.match(ledger,/Firm workspace · NXT native books/);assert.match(ledger,/data-qb-open-view="companies"/);assert.match(ledger,/pagehide/);
 for(const route of ['/api/ledger/accounts','/api/ledger/contacts','/api/ledger/invoices','/api/ledger/company'])assert.ok(ledger.includes(route));assert.ok(!code.includes('method:')&&!code.includes('POST')&&!code.includes('/api/ledger/'));
});
test('with openLedger a company choice opens its own ledger, including clients only Nixon lists',async()=>{
  const h=harness();const opened=[];
  h.setHandler(async path=>{if(path==='/api/nixon/clients')return {clients:[{client_id:'client-a',ledger_open:true}]};throw Object.assign(Error('firm-wide'),{status:403});});
  await h.mount({openLedger:id=>opened.push(id)}).ready;await new Promise(r=>setTimeout(r,0));
  assert.match(h.host.innerHTML,/client-a — Ledger open/);
  const select=h.host.querySelector('#ledger-company-choice');select.value='client-a';select.onchange();
  assert.deepEqual(opened,['client-a']);
});

test('shared api forwards cancellation without altering legacy GET/body behavior',async()=>{
 const calls=[];const box={window:{parent:null},sessionStorage:{getItem:()=> 'SYNTHETIC',setItem(){},removeItem(){}},fetch:async(path,init)=>{calls.push({path,init});return{ok:true,status:200,json:async()=>({ok:true})};},document:{querySelector(){return null;},querySelectorAll(){return[];},documentElement:{dataset:{}}},console};box.window.parent=box.window;
 vm.runInNewContext(read('public/nxt/ui.js'),box);const c=new AbortController();await box.window.NXT.api('/synthetic',{signal:c.signal});await box.window.NXT.api('/synthetic',{body:{value:1}});
 assert.equal(calls[0].init.signal,c.signal);assert.equal(calls[0].init.method,'GET');assert.equal(calls[1].init.method,'POST');assert.equal(calls[1].init.body,'{"value":1}');assert.equal(calls[1].init.signal,undefined);
});
test('Firm clients screen reads the guarded directory and states what is not loaded',()=>{
 const firm=read('public/apps/firm/firm.js');
 assert.match(firm,/api\('\/api\/clients'\)/);assert.match(firm,/Not in this directory/);assert.match(firm,/missing, not zero/);
});
test('Firm clients screen pages DT Max clients through the guarded route',()=>{
 const firm=read('public/apps/firm/firm.js');
 assert.match(firm,/test-copy\/clients\?offset=/);assert.match(firm,/DT Max clients/);assert.match(firm,/esc\(c\.name\)/);
});
test('staff sign-in asks for MFA, then a new password, then opens the workspace',()=>{
 const box={};vm.runInNewContext(read('public/nxt/staff-signin.js'),box);const {next}=box.NXTStaffSignin;
 assert.equal(next({mfa_enrollment_required:true,must_change_password:true}),'enroll');
 assert.equal(next({mfa_enrollment_required:false,must_change_password:true}),'password');
 assert.equal(next({mfa_enrollment_required:false,must_change_password:false}),'done');
 const home=read('public/index.html');assert.match(home,/api\/staff\/login/);assert.match(home,/Use the workspace key/);
 assert.match(read('public/apps/firm/firm.js'),/views\.staff=async/);
});

test('ledger-open flags never block or disable the company list (Nixon * timeout regression)',async()=>{
  const h=harness();h.setHandler(path=>path==='/api/nixon/clients'?new Promise(()=>{}):path.endsWith('/roster')?roster([{company_id:'co-a',company_name:'Synthetic A'}]):path==='/api/migrations'?source([]):page([]));
  await h.mount({openLedger:()=>{}}).ready;
  assert.match(h.host.innerHTML,/co-a/);assert.doesNotMatch(h.host.innerHTML,/<select[^>]*disabled/);
});
test('a timeout keeps companies that already arrived selectable',async()=>{
  const h=harness((path,{signal})=>path.endsWith('/roster')?Promise.resolve(roster([{company_id:'co-b',company_name:'Synthetic B'}])):new Promise((_,reject)=>signal.addEventListener('abort',()=>reject(Object.assign(Error('aborted'),{name:'AbortError'})),{once:true})));
  const m=h.mount({timeoutMs:5});await m.ready;assert.match(h.host.innerHTML,/co-b/);assert.doesNotMatch(h.host.innerHTML,/disabled/);m.destroy();
});
test('a client-scoped staff account gets a scope message, not a disabled picker, when firm-wide inventories are refused',async()=>{
 const denied=()=>{const e=Error('firm-wide views need an account with access to all clients');e.status=403;throw e;};
 const h=harness(path=>path.endsWith('/roster')||path.startsWith('/api/company-books')?denied():path==='/api/migrations'?source([]):{clients:[{client_id:'client-a',ledger_open:true}]});
 const m=h.mount({openLedger:()=>{}});await m.ready;await new Promise(r=>setTimeout(r,0));
 assert.ok(!/<select[^>]*disabled/.test(h.host.innerHTML));assert.match(h.host.innerHTML,/assigned clients/);assert.match(h.host.innerHTML,/value="client-a"/);m.destroy();
});
test('archived books are counted for pagination but not listed',async()=>{
 const h=harness();const archived={...book('B'),head:{company_id:'B',archived:true}};let n=0;
 const rows=await h.subject.booksRows(async()=>++n===1?page([book('A'),archived],3,0,true):page([book('C')],3,2,false),()=>true);
 assert.equal(rows.map(r=>r.company_id).join(),'A,C');
});
