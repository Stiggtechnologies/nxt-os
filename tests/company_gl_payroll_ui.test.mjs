import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const src=fs.readFileSync(new URL('../public/apps/migrations/migrations.js',import.meta.url),'utf8');
const ending="  window.addEventListener('hashchange',route);route();\n})();";
function load(){const n=()=>({innerHTML:'',dataset:{},querySelector:()=>null,querySelectorAll:()=>[],focus(){}});const sb={NXT:{api:async()=>{throw Error('no')},esc:v=>String(v??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';'),toast(){},modal:async()=>{},key:{get:()=>'k'}},document:{querySelector:n,getElementById:n,addEventListener(){}},location:{hash:'#x',origin:'https://s.invalid'},window:{addEventListener(){},confirm:()=>true},URLSearchParams,console,setTimeout};sb.window.parent=sb.window;
  vm.runInNewContext(src.replace(ending,'globalThis.subject={generalLedgerMarkup,payrollMarkup,routeInfo};})();'),sb);return sb.subject;}
test('general ledger tab renders one row per line with account names',()=>{
  const s=load();const html=s.generalLedgerMarkup({id:'synthetic-a'},{id:'synthetic-a',section:'general-ledger',reference:'',offset:0},{entries_total:1,lines_total:2,total:2,source_entries:1,source_lines:2,debit_total_cents:'500',credit_total_cents:'500',has_more:false,next_offset:null,rows:[
    {entry_id:'e',date:'2025-01-01',transaction_type:'Cheque',source_id:'r:1',source_row:1,account_id:'a1',account_name:'Payroll Expenses',memo:'',debit_cents:'500',credit_cents:'0',native:false},
    {entry_id:'e',date:'2025-01-01',transaction_type:'Cheque',source_id:'r:1',source_row:2,account_id:'a2',account_name:'Synthetic bank',memo:'',debit_cents:'0',credit_cents:'500',native:true}]});
  assert.equal((html.match(/<tr>/g)||[]).length,3);assert.match(html,/Payroll Expenses/);assert.match(html,/NXT/);
  assert.throws(()=>s.generalLedgerMarkup({id:'a'},{},{rows:null}));
});
test('payroll tab states journal basis and missing pay stubs',()=>{
  const s=load();const html=s.payrollMarkup({transactions_total:1,pay_stubs_available:false,note:'Derived from the committed journal.',payroll_accounts:[{name:'WCB',lines:1,debit_cents:'20255',credit_cents:'0'}],by_year:[{year:'2020',transactions:1,debit_cents:'20255',credit_cents:'0'}],transactions:[{date:'2020-03-01',transaction_type:'Cheque',source_id:'r:9',payroll_debit_cents:'20255',payroll_credit_cents:'0'}]});
  assert.match(html,/Not in this book/);assert.match(html,/WCB/);assert.match(html,/2020/);
});
