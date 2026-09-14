// Native acceptance driver for the compiled Rust Worker. All billing calls are
// made by the Rust suite through restate-e2e-harness ingress.
import { Miniflare } from "miniflare";
import { generateKeyPairSync } from "node:crypto";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:http";

const directory = await mkdtemp(join(tmpdir(),"szamlazz-workerd-"));
const {privateKey,publicKey} = generateKeyPairSync("ed25519");
const keyPath = join(directory,"identity.pem");
await writeFile(keyPath,privateKey.export({type:"pkcs8",format:"pem"}),{mode:0o600});
function base58(bytes) {
  const alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  let n = BigInt(`0x${Buffer.from(bytes).toString("hex")}`), result="";
  while(n) { result=alphabet[Number(n%58n)]+result; n/=58n; }
  for(const byte of bytes) { if(byte!==0) break; result="1"+result; }
  return result;
}
const identity = "publickeyv1_"+base58(publicKey.export({type:"spki",format:"der"}).subarray(-32));
let bindings = { SZAMLAZZ_NAMESPACE:"workers", RESTATE_IDENTITY_KEY:identity, SZAMLAZZ_ACCOUNTS:JSON.stringify({accounts:{
  alpha:{id:"alpha",agent_key:"WORKER-ALPHA-TEST-KEY",endpoint:process.env.MOCK_URL},
  beta:{id:"beta",agent_key:"WORKER-BETA-TEST-KEY",endpoint:process.env.MOCK_URL},
}}) };
const options = () => ({modules:true, modulesRules:[{type:"CompiledWasm",include:["**/*.wasm"]}],scriptPath:"build/worker/shim.mjs",compatibilityDate:"2026-07-30",bindings});
let workers = [new Miniflare(options()),new Miniflare(options())], exchanges=0;
let interrupt;
const server = createServer(async (req,res) => {
  try {
    if(req.url==="/__interrupt") {
      interrupt?.(); res.end("interrupted"); return;
    }
    if(req.url.startsWith("/__credentials/")) {
      bindings.ACCEPTANCE_CREDENTIALS=req.url.split("/").at(-1);
      await Promise.all(workers.map(w=>w.setOptions(options()))); res.end("updated"); return;
    }
    if(req.url==="/__replace") {
      await Promise.all(workers.map(w=>w.dispose())); workers=[new Miniflare(options()),new Miniflare(options())]; res.end("replaced"); return;
    }
    const chunks=[]; for await(const chunk of req) chunks.push(chunk);
    const body=Buffer.concat(chunks);
    const index=exchanges++%2;
    const execution=workers[index].dispatchFetch(`http://localhost${req.url}`,{method:req.method,headers:req.headers,...(body.length ? {body} : {})});
    let cut;
    const interrupted=new Promise(resolve=>{cut=resolve;});
    if(["create_invoice","create_proforma","delete_proforma"].some(handler=>req.url.includes(`/invoke/Szamlazz.Order/${handler}`))) interrupt=cut;
    const response=await Promise.race([execution,interrupted]);
    if(interrupt===cut) interrupt=undefined;
    if(!response) {
      const old=workers[index]; workers[index]=new Miniflare(options());
      // The replacement gets the unfinished journal. Disposal cancels the old
      // runtime; this is a worker-instance interruption, not a provider rollback.
      await old.dispose();
      await execution.catch(()=>{});
      res.writeHead(503);res.end("interrupted worker");return;
    }
    res.writeHead(response.status,Object.fromEntries(response.headers)); res.end(Buffer.from(await response.arrayBuffer()));
  } catch { res.writeHead(503); res.end("worker exchange failed"); }
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
console.log(JSON.stringify({uri:`http://127.0.0.1:${server.address().port}`,keyPath,identity}));
let stopping=false;
async function stop() {
  if(stopping) return; stopping=true;
  server.closeAllConnections(); await new Promise(resolve=>server.close(resolve));
  await Promise.all(workers.map(w=>w.dispose())); await rm(directory,{recursive:true});
}
process.on("SIGTERM",()=>stop().then(()=>process.exit(0)));
process.on("SIGINT",()=>stop().then(()=>process.exit(0)));
process.stdin.resume(); process.stdin.on("end",()=>stop().then(()=>process.exit(0)));
