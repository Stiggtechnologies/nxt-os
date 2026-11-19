import fs from 'node:fs';import path from 'node:path';import assert from 'node:assert/strict';import {fileURLToPath} from 'node:url';
const {chromium}=await import(process.env.NXT_PLAYWRIGHT_MODULE||'playwright');
const root=fileURLToPath(new URL('../public/',import.meta.url)),origin='https://nxt-esign.invalid';
const browser=await chromium.launch({headless:true,executablePath:process.env.NXT_CHROME_EXECUTABLE||undefined,chromiumSandbox:true});
try{
 const context=await browser.newContext({serviceWorkers:'block'}),calls=[],errors=[];
 await context.addInitScript(()=>sessionStorage.setItem('nxt_key','synthetic-workspace-key'));
 let connected=false,sent=false;
 const env={id:'synthetic-envelope',status:'approved',revision:1,content_hash:'synthetic-hash',draft:{title:'Synthetic document',recipients:[{name:'Synthetic Signer',email:'signer@example.invalid'}],documents:[{id:'doc',name:'Synthetic',pages:['Page 1']}],fields:[]}};
 await context.route('**/*',async route=>{
  const request=route.request(),u=new URL(request.url());if(u.origin!==origin)return route.abort();
  if(u.pathname.startsWith('/api/')){
   calls.push({path:u.pathname,method:request.method()});let value={};
   if(u.pathname==='/api/signatures/esign/catalog')value={templates:[],delivery_mode:connected?'smtp':'test'};
   else if(u.pathname==='/api/signatures/esign/mail'){
    if(request.method()==='PUT'){const body=request.postDataJSON();assert.equal(body.sender,'info@nxtcorporation.ca');assert.equal(body.password,'synthetic-mail-credential');assert.equal(body.port,465);connected=true;}
    value={configured:connected,secure_storage_ready:true,revision:connected?'saved-revision':'',enabled:connected,hostname:connected?'s1363.use1.mysecurecloudhost.com':null,sender:'info@nxtcorporation.ca'};
   }else if(u.pathname==='/api/signatures')value={envelopes:[{id:env.id,title:env.draft.title,status:env.status}]};
   else if(u.pathname.endsWith('/esign/send')){sent=true;value={delivery:{mode:'smtp',email_sent:true,receipts:[{recipient_id:'signer',receipt:{status:'accepted_by_mail_server'}}],links:[]}};}
   else if(u.pathname.endsWith('/esign'))value={status:sent?'sent':'prepared',delivery_mode:connected?'smtp':'test',signers:[],events:[],pdf_sha256:null};
   else if(u.pathname==='/api/signatures/'+env.id)value={envelope:env};
   return route.fulfill({contentType:'application/json',body:JSON.stringify(value)});
  }
  const file=path.resolve(root,'.'+u.pathname);assert.ok(file.startsWith(root));if(!fs.existsSync(file))return route.fulfill({status:404,body:''});
  return route.fulfill({contentType:file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html',body:fs.readFileSync(file)});
 });
 const page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));
 await page.goto(origin+'/apps/signatures/esign.html?company=synthetic-a#mail');
 await page.getByRole('heading',{name:'Sending mailbox'}).waitFor();
 await page.locator('[name=hostname]').selectOption('s1363.use1.mysecurecloudhost.com');await page.locator('[name=port]').selectOption('465');await page.locator('#smtp-password').fill('synthetic-mail-credential');await page.locator('[name=enabled]').check();
 await page.getByRole('button',{name:'Save and check connection'}).click();await page.getByText('TLS and mailbox authentication verified.',{exact:false}).waitFor();
 assert.equal(await page.locator('#smtp-password').inputValue(),'');assert.equal(await page.evaluate(()=>Object.values(localStorage).join('|').includes('synthetic-mail-credential')),false);
 assert.equal(calls.some(c=>c.path.endsWith('/esign/send')),false,'checking connection must send no document');
 await page.evaluate(()=>location.hash='envelope/synthetic-envelope');
 await page.getByRole('heading',{name:'Synthetic document',exact:true}).waitFor();
 await page.getByRole('button',{name:'Email signature request'}).click();
 await page.getByRole('heading',{name:'Send signature request'}).waitFor();
 assert.equal(sent,false,'a visible confirmation must precede send');
 assert.match(await page.locator('.modal-back').innerText(),/signer@example.invalid/);
 await page.getByRole('button',{name:'Confirm request'}).click();
 await page.getByText('Accepted by the mail server. Inbox delivery is not yet confirmed.',{exact:true}).waitFor();
 assert.equal(calls.filter(c=>c.path.endsWith('/esign/send')).length,1);
 assert.equal(new URL(page.url()).searchParams.get('company'),'synthetic-a');assert.deepEqual(errors,[]);
 console.log(JSON.stringify({passed:true,password_field_cleared:true,no_browser_credential_storage:true,connection_check_sends_no_mail:true,explicit_send_confirmation:true,recipient_visible:true,one_send:true,company_preserved:true,provider:'synthetic'}));
}finally{await browser.close();}
