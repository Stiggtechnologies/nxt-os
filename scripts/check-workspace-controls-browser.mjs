import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
// Optional browser acceptance test. Provide Playwright and its Chromium browser,
// or set NXT_PLAYWRIGHT_MODULE / NXT_CHROME_EXECUTABLE for an existing installation.
const { chromium } = await import(process.env.NXT_PLAYWRIGHT_MODULE || "playwright");

const root = path.resolve(fileURLToPath(new URL("../public", import.meta.url)));
const origin = "https://nxt-navigation.invalid";
const browser = await chromium.launch({
  executablePath: process.env.NXT_CHROME_EXECUTABLE || undefined,
  headless: true, chromiumSandbox: true,
});
const errors = [], requests = [];
try {
  const context = await browser.newContext({ serviceWorkers: "block", viewport: {width:1440,height:1000} });
  await context.addInitScript(() => {
    sessionStorage.setItem("nxt_key", "synthetic-key");
    if (window !== window.top) return;
    window.proof = {peers:[],channels:[],sent:[],microphones:0,stopped:0};
    window.RTCPeerConnection = class {
      constructor() { this.closed=false; proof.peers.push(this); }
      close() { this.closed=true; }
      addTrack() {}
      createDataChannel() {
        this.channel={readyState:"connecting",send:message => proof.sent.push(JSON.parse(message))};
        proof.channels.push(this.channel);
        return this.channel;
      }
      async createOffer() { return {sdp:"synthetic-offer"}; }
      async setLocalDescription() {}
      async setRemoteDescription() {
        this.channel.readyState="open";
        setTimeout(() => this.channel.onopen?.(), 0);
      }
    };
    navigator.mediaDevices.getUserMedia = async () => {
      proof.microphones++;
      return {getTracks:() => [{stop:() => {proof.stopped++;}}]};
    };
  });
  await context.route("**/*", async route => {
    const request=route.request(), url=new URL(request.url()), p=url.pathname;
    if(url.origin!==origin) return route.fulfill({body:""});
    if(p.startsWith("/api/")){
      const body=request.postDataJSON();
      requests.push({path:p,body});
      let value={};
      if(p==="/api/os/status") value={auth_required:true,ai_enabled:true};
      if(p==="/api/staff/me") value={identity:{role:"admin",clients:["*"],display_name:"Synthetic"},display_name:"Synthetic"};
      if(p==="/api/clients" || p==="/api/nixon/clients") value={clients:[{client_ref:"synthetic-alpha",name:"Synthetic Alpha"},{client_ref:"synthetic-beta",name:"Synthetic Beta"}]};
      if(p==="/api/nixon/status") value={posting:"disabled",filing:"disabled",surfaces:{}};
      if(p==="/api/nixon/reviews") value=[];
      if(p==="/api/ledger/meta") value={today:"2026-11-19"};
      if(p==="/api/nixon/gods-eye") value={companies:[],watchers:[],findings:[],sources:[],ai:{enabled:false},guardrails:{books:"read_only"},as_of:"synthetic"};
      if(p==="/api/nixon/voice/realtime") value={transport:{sdp:"synthetic-answer"}};
      if(p==="/api/os/ask") value={reply:"Synthetic reply "+body.messages.at(-1).content};
      if(p==="/api/nixon/voice") value={intent:"navigate",action:"speak",say:"Opened ledger",open:{app:"ledger",view:"journal"}};
      return route.fulfill({contentType:"application/json",body:JSON.stringify(value)});
    }
    // Real shell and Nixon page; other apps are frame fixtures for the routing lifecycle.
    if(/^\/apps\/(ledger|tax|firm)\/$/.test(p)) {
      return route.fulfill({contentType:"text/html",body:`<!doctype html><title>App fixture</title><h1>App fixture</h1>
        <nav><button aria-label="Tasks" onclick="document.querySelector('h1').textContent='Tasks opened'">▤</button><a href="#reports">Reports</a></nav>
        <form onsubmit="event.preventDefault();document.querySelector('#saved').textContent='Draft saved'">
        <label>Customer name<input name="customer"></label><label>Tax year<select name="year"><option value="2024">2024</option><option value="2025">2025</option></select></label>
        <label><input type="checkbox" name="include">Include prior year</label><button>Save draft</button></form>
        <button disabled>Disabled control</button><button hidden>Hidden control</button><input type="password" aria-label="Secret password"><button title="Duplicate">Duplicate</button><button title="Duplicate">Duplicate</button>
        <a href="https://outside.invalid/">External link</a><p id="saved"></p>
        <button onclick="this.remove()">Remove this control</button>
        <table><tr><td>Record Alpha</td><td><button>Open</button></td></tr><tr><td>Record Beta</td><td><button>Open</button></td></tr></table>
        <script>document.addEventListener('keydown',e=>{if(e.key==='F9'){e.preventDefault();document.querySelector('#saved').textContent='Shortcut handled';}});</script><script src="/nxt/ui.js"></script>`});
    }
    const file=path.resolve(root,"."+ (p==="/" ? "/index.html" : p.endsWith("/") ? p+"index.html" : p));
    assert.ok(file.startsWith(root+path.sep));
    if(!fs.existsSync(file))return route.fulfill({status:404,body:""});
    return route.fulfill({contentType:file.endsWith(".js")?"text/javascript":file.endsWith(".css")?"text/css":"text/html",body:fs.readFileSync(file)});
  });
  const page=await context.newPage();
  page.on("pageerror",error => errors.push(error.message));
  await page.goto(origin+"/#/nixon/gods-eye");
  const nixon=page.frameLocator("#frame");
  await nixon.locator("#liveVoice").click();
  await page.locator('#chat .nxc-voice[aria-pressed="true"]').waitFor();
  await page.waitForFunction(() => proof.channels[0]?.readyState==="open");
  let number=0;
  const tool=async(name,args)=>{
    const id="workspace-"+(++number);
    await page.evaluate(async ({name,args,id}) => {
      await proof.channels[0].onmessage({data:JSON.stringify({type:"response.done",response:{output:[
        {type:"function_call",call_id:id,name,arguments:JSON.stringify(args)}
      ]}})});
    },{name,args,id});
    return page.evaluate(id=>JSON.parse(proof.sent.find(item=>item.item?.call_id===id).item.output),id);
  };
  const company=(await tool("inspect_workspace",{query:"company"})).controls.find(c=>c.label==="Company");
  assert.ok(company);
  const dropdown=await tool("control_workspace",{action:"click",target:company.id});
  assert.equal(dropdown.options.length,3);
  await page.getByRole("dialog",{name:"Company options"}).waitFor();
  assert.equal((await tool("control_workspace",{action:"select",target:company.id,value:"synthetic-beta"})).ok,true);
  await page.waitForFunction(()=>new URL(location.href).searchParams.get("company")==="synthetic-beta");
  assert.equal((await tool("control_workspace",{action:"click",target:company.id})).ok,false,"old handles expire on company changes");
  assert.equal((await tool("control_workspace",{action:"navigate",target:"Firm · Projects"})).ok,true);
  await page.waitForFunction(()=>location.hash==="#/firm/projects");
  await page.frameLocator("#frame").getByRole("heading",{name:"App fixture"}).waitFor();
  let controls=(await tool("inspect_workspace",{})).controls;
  assert.ok(controls.some(c=>c.label==="Tasks"));
  assert.ok(!controls.some(c=>/Secret password|Hidden control/.test(c.label)));
  const byLabel=label=>controls.find(c=>c.label===label)?.id;
  assert.equal((await tool("control_workspace",{action:"click",target:byLabel("Tasks")})).ok,true);
  assert.equal(await page.frameLocator("#frame").locator("h1").innerText(),"Tasks opened");
  assert.equal((await tool("control_workspace",{action:"fill",target:byLabel("Customer name"),value:"Synthetic Customer"})).ok,true);
  assert.equal(await page.frameLocator("#frame").locator('[name="customer"]').inputValue(),"Synthetic Customer");
  assert.equal((await tool("control_workspace",{action:"select",target:byLabel("Tax year"),value:"2025"})).ok,true);
  assert.equal((await tool("control_workspace",{action:"check",target:byLabel("Include prior year"),value:"true"})).ok,true);
  assert.equal((await tool("control_workspace",{action:"click",target:byLabel("Save draft")})).ok,true);
  assert.equal(await page.frameLocator("#frame").locator("#saved").innerText(),"Draft saved");
  assert.equal((await tool("control_workspace",{action:"press",target:byLabel("Customer name"),value:"F9"})).ok,true);
  assert.equal(await page.frameLocator("#frame").locator("#saved").innerText(),"Shortcut handled");
  assert.equal((await tool("control_workspace",{action:"press",target:byLabel("Customer name"),value:"Ctrl+l"})).ok,false);
  assert.equal((await tool("control_workspace",{action:"press",target:byLabel("Customer name"),value:"Tab"})).ok,true);
  await page.frameLocator("#frame").locator('[name="year"]:focus').waitFor();
  assert.equal(await page.frameLocator("#frame").locator('[name="year"]').evaluate(el=>el===document.activeElement),true,await page.frameLocator("#frame").locator('body').evaluate(()=>document.activeElement.outerHTML));
  const rowControls=await tool("inspect_workspace",{query:"Record Beta"});
  assert.equal(rowControls.controls.filter(c=>c.label==="Open").length,1);
  assert.match(rowControls.controls.find(c=>c.label==="Open").context,/Record Beta/);
  assert.equal((await tool("control_workspace",{action:"click",target:byLabel("Disabled control")})).ok,false);
  assert.equal((await tool("control_workspace",{action:"click",target:"Duplicate"})).choices.length,2);
  assert.equal((await tool("control_workspace",{action:"click",target:byLabel("External link")})).ok,false);
  const removed=byLabel("Remove this control");
  assert.equal((await tool("control_workspace",{action:"click",target:removed})).ok,true);
  assert.equal((await tool("control_workspace",{action:"click",target:removed})).ok,false);
  // Browser speech and typed commands use the same UI bridge without a cloud round trip.
  await page.locator("#chat textarea").fill("Nixon, switch company to Synthetic Alpha");
  await page.locator("#chat .nxc-composer").evaluate(form=>form.requestSubmit());
  await page.waitForFunction(()=>new URL(location.href).searchParams.get("company")==="synthetic-alpha");
  await page.evaluate(() => { location.hash="/nixon"; });
  await page.waitForFunction(()=>location.hash==="#/nixon");
  await page.frameLocator("#frame").locator("#liveVoice").click();
  await page.evaluate(async () => {
    await proof.channels[0].onmessage({data:JSON.stringify({type:"response.output_audio_transcript.done",transcript:"I am still here on Nixon."})});
  });
  assert.match(await page.locator("#chat .nxc-log").innerText(),/Synthetic Alpha/);
  assert.match(await page.locator("#chat .nxc-log").innerText(),/I am still here on Nixon/);
  const proof=await page.evaluate(() => ({
    peers:window.proof.peers.length,microphones:window.proof.microphones,stopped:window.proof.stopped,
    closed:window.proof.peers[0].closed,sent:window.proof.sent,
  }));
  assert.equal(proof.peers,1);
  assert.equal(proof.microphones,1);
  assert.equal(proof.stopped,0);
  assert.equal(proof.closed,false);
  assert.ok(proof.sent.some(item=>item.item?.content?.[0]?.text.includes("/firm/projects")));
  assert.equal(requests.filter(r=>r.path==="/api/nixon/voice/realtime").length,1);
  await page.locator("#signOut").click();
  await page.waitForFunction(() => proof.stopped===1);
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({passed:true,tool_calls:number,sequence:"company dropdown → choose company → Firm tasks → fill/select/check/save → stale/disabled/ambiguous guards → typed company switch → Nixon → sign-out",microphone_sessions:1,realtime_sessions:1,actual_shell_and_nixon:true,synthetic_transport:true}));
}finally{await browser.close();}
