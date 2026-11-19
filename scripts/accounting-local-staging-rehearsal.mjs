import { createHmac, randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { mkdirSync, statSync } from "node:fs";
import { createServer } from "node:net";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../",import.meta.url));
const need = (ok,message) => { if (!ok) throw new Error(message); };
const password = () => randomBytes(32).toString("base64url");
function totp(secret) {
  let bits = 0, value = 0;
  const decoded = [];
  const charset = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  for (const char of secret) {
    const digit = charset.indexOf(char);
    need(digit >= 0,"Invalid synthetic MFA seed");
    value = (value << 5) | digit;
    bits += 5;
    if (bits >= 8) { bits -= 8; decoded.push((value >> bits) & 255); }
  }
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));
  const mac = createHmac("sha1",Buffer.from(decoded)).update(counter).digest();
  const offset = mac.at(-1) & 15;
  return String((mac.readUInt32BE(offset)&0x7fffffff)%1_000_000).padStart(6,"0");
}
async function availablePort() {
  const socket = createServer();
  await new Promise((ok,fail)=>{socket.once("error",fail);socket.listen(0,"127.0.0.1",ok);});
  const port = socket.address().port;
  await new Promise((ok,fail)=>socket.close(error=>error?fail(error):ok()));
  return port;
}
const [binary,directory] = process.argv.slice(2);
let child;
try {
  need(process.argv.length === 4 && statSync(binary).isFile(),"Pass the NXT dev binary and a new local rehearsal directory");
  const path = resolve(directory);
  mkdirSync(path,{mode:0o700});
  const store = join(path,"isolated-store.jsonl");
  const port = await availablePort();
  const base = "http://127.0.0.1:"+port;
  const bootstrap = password();

  async function api(route,{ token, body, status = 200 } = {}) {
    const response = await fetch(base+route,{
      method:body===undefined?"GET":"POST",redirect:"error",signal:AbortSignal.timeout(4_000),
      headers:{...(token?{authorization:"Bearer "+token}:{}),"content-type":"application/json"},
      ...(body===undefined?{}:{body:JSON.stringify(body)}),
    });
    need(response.status===status,`Unexpected status at ${route}: HTTP ${response.status}`);
    return response.json();
  }
  async function start(bootstrapEnabled) {
    child = spawn(resolve(binary),[],{
      cwd:root,windowsHide:true,stdio:"ignore",
      env:{
        ...Object.fromEntries(Object.entries(process.env).filter(([key])=>
          ["PATH","SYSTEMROOT","WINDIR"].includes(key.toUpperCase()))),
        PORT:String(port), NXT_DEV_STORE_FILE:store, NXT_ACCOUNTING_EXTENSION:"on",
        ADMIN_API_KEY:bootstrap,NXT_BREAK_GLASS:bootstrapEnabled?"on":"off",NXT_STAFF_REQUIRE_MFA:"on",
        NXT_STAFF_PBKDF2_ITERATIONS:"10000",
      },
    });
    let startError;
    child.once("error",error=>{startError=error;});
    for (let attempt=0;attempt<30;attempt++) {
      if (startError) throw startError;
      need(child.exitCode===null,"Synthetic staging server exited during startup");
      try { if ((await api("/api/health")).ok) return; } catch {}
      await new Promise(ok=>setTimeout(ok,250));
    }
    throw new Error("Synthetic staging server did not respond");
  }
  async function stop() {
    if (!child || child.exitCode!==null || child.signalCode!==null) return;
    const stopped = new Promise(ok=>child.once("exit",ok));
    child.kill();
    await stopped;
  }
  await start(true);
  const accounts = [];
  for (const [username,role,clients] of [
    ["staging-admin-a","admin",["*"]],["staging-admin-b","admin",["*"]],
    ["staging-preparer","preparer",["stage-fixtures"]],["staging-reviewer","reviewer",["stage-fixtures"]],
  ]) {
    const temporary_password = password();
    const new_password = password();
    await api("/api/staff/users",{token:bootstrap,body:{username,display_name:username,role,clients,temporary_password}});
    const {token} = await api("/api/staff/login",{body:{username,password:temporary_password}});
    await api("/api/staff/me/password",{token,body:{current_password:temporary_password,new_password}});
    const {secret} = await api("/api/staff/mfa/enroll",{token,body:{}});
    await api("/api/staff/mfa/confirm",{token,body:{code:totp(secret)}});
    accounts.push({username,role,token});
  }
  await stop();
  await start(false);
  const ids = new Set();
  for (const account of accounts) {
    const {identity} = await api("/api/staff/me",{token:account.token});
    need(identity.mfa && !identity.break_glass && identity.role===account.role,"Staging identity failed persistence or role check");
    ids.add(identity.user_id);
  }
  need(ids.size===accounts.length,"Staging identities are not distinct");
  await api("/api/staff/users",{token:bootstrap,status:401});
  const readiness = await api("/api/accounting-extension/readiness",{token:accounts[0].token});
  need(readiness.live_writes_enabled===false && readiness.production_dispatch_available===false,"Live dispatch is not disabled");
  await api("/api/accounting-extension/readiness",{token:accounts[2].token,status:403});
  need(statSync(store).size>0,"No durable isolated store was written");
  console.log(JSON.stringify({
    target:"local_loopback_rehearsal",persistent_after_restart:true,
    separate_mfa_accounts:accounts.map(({username,role})=>({username,role})),
    bootstrap_disabled_after_restart:true, accounting_extension_enabled:true,
    live_writes_enabled:false,hosted_acceptance_cases_executed:0,
  },null,2));
} catch(error) {
  console.error(error instanceof Error?error.message:"Isolated staging rehearsal failed");
  process.exitCode=1;
} finally {
  if (child && child.exitCode===null && child.signalCode===null) {
    const stopped = new Promise(ok=>child.once("exit",ok));
    child.kill();
    await stopped;
  }
}
