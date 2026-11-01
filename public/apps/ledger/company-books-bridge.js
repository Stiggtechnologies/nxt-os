/* Company navigation reads metadata only. It never copies records into Ledger. */
(() => {
  'use strict';
  const validId=value=>typeof value==='string'&&/^[A-Za-z0-9][A-Za-z0-9_.-]{0,95}$/.test(value);
  const hash=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
  const name=value=>typeof value==='string'&&value.trim()&&value.length<=300;
  const need=(condition,message)=>{if(!condition)throw Error(message);};
  function rosterRows(value){
    need(value&&Array.isArray(value.companies)&&value.companies.length<=1000&&value.count===value.companies.length&&value.financial_imports_inferred===false&&value.inventory_complete===false,'Observed company inventory is incomplete.');
    const ids=new Set();
    for(const row of value.companies){need(row&&validId(row.company_id)&&name(row.company_name)&&!ids.has(row.company_id),'Observed company references are invalid or duplicated.');ids.add(row.company_id);}
    return value.companies;
  }
  function sourceRows(value){
    need(value?.shadow_only===true&&value.source_authoritative===true&&value.live_cutover_allowed===false&&Array.isArray(value.snapshots)&&value.snapshots.length<=500,'Saved source inventory is incomplete.');
    const ids=new Set();
    for(const row of value.snapshots){need(row&&validId(row.id)&&validId(row.cohort_ref)&&name(row.company_name)&&!ids.has(row.id),'Saved source references are invalid or duplicated.');ids.add(row.id);}
    return value.snapshots;
  }
  async function booksRows(request,isCurrent){
    const rows=[],ids=new Set();let offset=0,total;
    for(let page=0;page<11;page++){
      const value=await request('/api/company-books?offset='+offset+'&limit=50');
      if(!isCurrent())return null;
      need(value?.controls?.source_authoritative===true&&value.controls.parallel_test===true&&value.controls.live_cutover_allowed===false,'Company book authority is unavailable.');
      need(Array.isArray(value.companies)&&value.companies.length<=50&&Number.isSafeInteger(value.total)&&value.total>=0&&value.total<=500&&value.offset===offset&&typeof value.has_more==='boolean'&&(total===undefined||total===value.total),'Company book inventory changed or is incomplete.');
      total=value.total;
      for(const row of value.companies){
        need(row&&validId(row.company_id)&&!ids.has(row.company_id),'Company book references are invalid or duplicated.');
        need(row.head===null?row.stage==='staged':row.head&&row.head.company_id===row.company_id&&hash(row.head_sha256)&&row.stage==='import_committed','Company journal status is inconsistent.');
        ids.add(row.company_id);rows.push(row);
      }
      need(rows.length<=total,'Company book inventory exceeds its recorded total.');
      if(!value.has_more){need(rows.length===total,'Company book inventory ended early.');return rows;}
      need(value.companies.length>0&&rows.length<total,'Company book pagination did not advance.');offset+=value.companies.length;
    }
    throw Error('Company book inventory exceeds its supported bound.');
  }
  function join(roster,sources,books){
    const rows=new Map();
    const get=(id,label)=>{if(!rows.has(id))rows.set(id,{id,name:label||id,observed:false,sources:0,book:null});return rows.get(id);};
    for(const row of roster||[])get(row.company_id,row.company_name).observed=true;
    for(const row of sources||[])get(row.cohort_ref,row.company_name).sources++;
    for(const row of books||[]){const label=row.previews?.[0]?.company?.name;get(row.company_id,name(label)?label:row.company_id).book=row;}
    return [...rows.values()].sort((a,b)=>Number(!!b.book?.head)-Number(!!a.book?.head)||a.name.localeCompare(b.name)||a.id.localeCompare(b.id));
  }
  function status(row,available){
    if(row.book?.head)return 'Journal imported';
    if(row.book)return 'Journal assessment saved';
    if(!available.books)return row.sources?'Reports saved · journal status unavailable':'Journal status unavailable';
    if(row.sources)return 'Reports saved · no journal import';
    return available.sources?'Observed only · no published records':'Source status unavailable · no journal import';
  }
  function companyView(id,section=''){
    need(validId(id)&&['','trial-balance','entries','customers','suppliers'].includes(section),'Select a valid company view.');
    return 'company/'+encodeURIComponent(id)+(section?'/'+section:'');
  }
  function mount(host,{api,esc,openApp,timeoutMs=20000}={}){
    need(Number.isFinite(timeoutMs)&&timeoutMs>0&&timeoutMs<=20000,'Company directory timeout exceeds its supported bound.');
    let disposed=false,version=0,controller=null,timer=null,rows=[],available={books:false,sources:false},message='Loading company directory…';
    const live=v=>!disposed&&host.isConnected!==false&&version===v;
    function stop(){controller?.abort();if(timer!==null)clearTimeout(timer);timer=null;}
    function navigate(view){if(disposed)return;stop();disposed=true;openApp('migrations',view);}
    function draw(v,loading=false,auth=false){
      if(!live(v))return;
      host.innerHTML=`<label for="ledger-company-choice">Company books</label><select id="ledger-company-choice" aria-describedby="ledger-company-scope"${loading||auth?' disabled':''}><option value="">Firm workspace · NXT native books</option>${rows.map(row=>`<option value="${esc(row.id)}">${esc(row.name)} — ${esc(status(row,available))}</option>`).join('')}</select><p id="ledger-company-scope">Current screen: Firm workspace. Select a company to open its own accounts, journals and saved contacts.</p>${message?`<p class="company-directory-message" role="${auth?'alert':'status'}">${esc(message)}</p>`:''}<a href="/#/migrations/companies" data-company-directory>View all companies</a>${!loading?'<button class="btn sm" type="button" data-company-reload>Refresh companies</button>':''}`;
      const select=host.querySelector('#ledger-company-choice');
      if(select)select.onchange=()=>{if(!live(v)||!select.value)return;const row=rows.find(row=>row.id===select.value);if(!row)return;navigate(companyView(row.id,row.book?.head?'trial-balance':''));};
      const directory=host.querySelector('[data-company-directory]');if(directory)directory.onclick=event=>{event.preventDefault();if(live(v))navigate('companies');};
      const retry=host.querySelector('[data-company-reload]');if(retry)retry.onclick=()=>{if(live(v))reload();};
    }
    async function reload(){
      stop();const v=++version;const requestController=new AbortController();controller=requestController;const signal=requestController.signal;
      rows=[];available={books:false,sources:false};message='Loading company directory…';draw(v,true);
      timer=setTimeout(()=>requestController.abort(),timeoutMs);
      const request=path=>api(path,{signal}).catch(error=>{if(error.status===401)requestController.abort();throw error;});
      try{
        const result=await Promise.allSettled([
          request('/api/company-books/roster').then(rosterRows),
          request('/api/migrations').then(sourceRows),
          booksRows(request,()=>live(v))
        ]);
        if(!live(v))return;
        if(result.some(r=>r.status==='rejected'&&r.reason?.status===401)){rows=[];message='Sign in to view company books. Company coverage is unavailable.';draw(v,false,true);return;}
        if(signal.aborted){rows=[];message='Company directory timed out. Refresh to check company coverage.';draw(v);return;}
        const values=result.map(r=>r.status==='fulfilled'?r.value:null);
        available={sources:values[1]!==null,books:values[2]!==null};rows=join(...values);
        const missing=result.filter(r=>r.status==='rejected').length;
        message=missing?'Some company status is unavailable. No missing data is treated as an empty company.':rows.length?'Company journals and saved source contacts open in their own company views.':'No company directory records are published. This does not establish an empty source.';
        draw(v);
      }catch{if(live(v)){rows=[];message='Company directory unavailable. Refresh or sign in again.';draw(v);}}
      finally{if(live(v)&&timer!==null){clearTimeout(timer);timer=null;}}
    }
    const ready=reload();
    return {ready,reload,destroy(){disposed=true;version++;stop();}};
  }
  globalThis.NXTLedgerCompanyBridge={mount,companyView,join,status,rosterRows,sourceRows,booksRows};
})();
