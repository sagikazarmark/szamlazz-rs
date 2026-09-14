import assert from "node:assert/strict";
import { test } from "node:test";
import { Miniflare } from "miniflare";
import { createServer } from "node:http";
// Syntactically valid Ed25519 public key; discovery doesn't require its signature.
const identity = "publickeyv1_" + "1".repeat(32);

test("actual endpoint rejects unsigned discovery in workerd", async () => {
  const mf = new Miniflare({
    modules: true,
    modulesRules: [{type:"CompiledWasm",include:["**/*.wasm"]}],
    scriptPath: "build/worker/shim.mjs",
    compatibilityDate: "2026-07-30",
    bindings: { SZAMLAZZ_NAMESPACE:"workers", RESTATE_IDENTITY_KEY:identity, SZAMLAZZ_ACCOUNTS: JSON.stringify({account:{id:"test",agent_key:"NOT-A-REAL-KEY"}}) },
  });
  try {
    const response = await mf.dispatchFetch("http://localhost/discover", {headers:{accept:"application/vnd.restate.endpointmanifest.v4+json"}});
    assert.equal(response.status, 401);
  } finally { await mf.dispose(); }
});

test("complete-response deadline includes a stalled body and aborts the transfer", {timeout:75000}, async () => {
  let sends=0, closed=false;
  const server=createServer((req,res)=>{
    sends++; req.resume(); res.on("close",()=>{closed=true;});
    res.writeHead(200,{"content-type":"application/xml",szlahu_error_code:"7"}); res.write("<incomplete>");
  });
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const mf=new Miniflare({modules:true,modulesRules:[{type:"CompiledWasm",include:["**/*.wasm"]}],scriptPath:"build/worker/shim.mjs",compatibilityDate:"2026-07-30",
    bindings:{SZAMLAZZ_ACCOUNTS:JSON.stringify({account:{id:"test",agent_key:"NOT-A-REAL-KEY",endpoint:`http://127.0.0.1:${server.address().port}/stall`}})}});
  try {
    const start=Date.now();
    const response=await mf.dispatchFetch("http://localhost/transport-probe");
    assert.equal(response.status,200,await response.clone().text());
    assert.deepEqual((await response.json()).accepted,[false],"headers alone cannot establish acceptance");
    assert.ok(Date.now()-start>=59000 && Date.now()-start<70000);
    await new Promise(resolve=>setTimeout(resolve,100));
    assert.equal(sends,1,"no HTTP resend"); assert.equal(closed,true,"deadline cancels the body transfer");
  } finally { await mf.dispose();server.closeAllConnections();await new Promise(resolve=>server.close(resolve)); }
});

test("each exchange reauthenticates and never reuses response session cookies", async () => {
  const seen=[];
  const server=createServer(async (req,res)=>{
    const chunks=[];for await(const chunk of req)chunks.push(chunk);
    seen.push({cookie:req.headers.cookie,body:Buffer.concat(chunks).toString()});
    res.writeHead(200,{"set-cookie":["OTHER=not-session; Path=/","JSESSIONID=session-test; Path=/"],szlahu_error_code:"7"});res.end();
  });
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const options=(key)=>({modules:true,modulesRules:[{type:"CompiledWasm",include:["**/*.wasm"]}],scriptPath:"build/worker/shim.mjs",compatibilityDate:"2026-07-30",
    bindings:{SZAMLAZZ_ACCOUNTS:JSON.stringify({account:{id:"test",agent_key:key,endpoint:`http://127.0.0.1:${server.address().port}/session`}})}});
  const mf=new Miniflare(options("FIRST-TEST-KEY"));
  try {
    for(const key of ["FIRST-TEST-KEY","SECOND-TEST-KEY"]) {
      await mf.setOptions(options(key));
      const response=await mf.dispatchFetch("http://localhost/transport-probe?repeat");
      assert.equal(response.status,200,await response.clone().text());assert.deepEqual((await response.json()).accepted,[true,true]);
    }
    assert.equal(seen.length,4);
    for(let i=0;i<seen.length;i++) {assert.equal(seen[i].cookie,undefined);assert.ok(seen[i].body.includes(i<2?"FIRST-TEST-KEY":"SECOND-TEST-KEY"));}
  } finally {await mf.dispose();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
});

test("comma-containing metadata does not override the shared parser's verdict", async () => {
  const server=createServer((req,res)=>{req.resume();res.writeHead(200,{szlahu_error_code:"7",szlahu_brutto:"1270,00"});res.end();});
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const mf=new Miniflare({modules:true,modulesRules:[{type:"CompiledWasm",include:["**/*.wasm"]}],scriptPath:"build/worker/shim.mjs",compatibilityDate:"2026-07-30",
    bindings:{SZAMLAZZ_ACCOUNTS:JSON.stringify({account:{id:"test",agent_key:"NOT-A-REAL-KEY",endpoint:`http://127.0.0.1:${server.address().port}/headers`}})}});
  try {const response=await mf.dispatchFetch("http://localhost/transport-probe");assert.deepEqual((await response.json()).accepted,[true]);}
  finally {await mf.dispose();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
});

test("dropping an exchange aborts pending transfer before its HTTP deadline", async () => {
  let closed=false,sends=0;
  const server=createServer((req,res)=>{sends++;req.resume();res.on("close",()=>{closed=true;});res.writeHead(200);res.write("pending");});
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const mf=new Miniflare({modules:true,modulesRules:[{type:"CompiledWasm",include:["**/*.wasm"]}],scriptPath:"build/worker/shim.mjs",compatibilityDate:"2026-07-30",
    bindings:{SZAMLAZZ_ACCOUNTS:JSON.stringify({account:{id:"test",agent_key:"NOT-A-REAL-KEY",endpoint:`http://127.0.0.1:${server.address().port}/cancel`}})}});
  try {const start=Date.now();const response=await mf.dispatchFetch("http://localhost/transport-probe?cancel");assert.equal((await response.json()).cancelled,true);
    await new Promise(resolve=>setTimeout(resolve,100));assert.equal(closed,true);assert.equal(sends,1);assert.ok(Date.now()-start<5000);}
  finally {await mf.dispose();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
});
