import assert from "node:assert/strict";
import test from "node:test";
import { ConsoleClient } from "@lenso/console";
import { ActionError } from "@lenso/app";
import { z } from "zod";
const description = { version: 1, modules: [{id:"work",actions:["work.list"],tasks:[],events:[],resources:[]}], pages:[{id:"work",title:"Work",list:"work.list"}] };
const client = (value) => new ConsoleClient("https://example.test/console", () => "token", async () => Response.json(value));

test("parsed results distinguish response failures from user input and preserve server errors", async () => {
  const schema = z.object({ id: z.string() });
  assert.deepEqual(await client({id:"one"}).callParsed("work.write", schema, {}), {id:"one"});
  await assert.rejects(client({wrong:true}).callParsed("work.write",schema,{}), (error) => {
    assert.equal(error.code,"internal"); assert.ok(error.cause instanceof z.ZodError); return true;
  });
  const denied = new ConsoleClient("https://example.test/console",()=>"token",async()=>Response.json({error:{code:"forbidden",message:"Denied"}},{status:403}));
  await assert.rejects(denied.callParsed("work.write",schema,{}),{code:"forbidden",message:"Denied"});
  await assert.rejects(denied.manifest(),{code:"forbidden",message:"Denied"});
  await assert.rejects(client({}).callParsed("work.write",{async parse(){throw new Error("Rejected result");}}),{code:"internal"});
});

test("cancelled Console calls do not start credential acquisition or dispatch", async () => {
  let credentials=0, requests=0;
  const transport = new ConsoleClient("https://example.test/console",async()=>{credentials++; return "token";},async()=>{requests++;return Response.json(null);});
  const controller = new AbortController(), reason = new Error("Obsolete page"); controller.abort(reason);
  await assert.rejects(transport.call("work.list",null,controller.signal),(error)=>error===reason);
  assert.equal(credentials,0); assert.equal(requests,0);
});

test("cancellation during token acquisition prevents later request dispatch", async () => {
  let release; const token = new Promise((resolve)=>{release=resolve;}); let requests=0;
  const transport = new ConsoleClient("https://example.test/console",()=>token,async()=>{requests++;return Response.json(null);});
  const controller = new AbortController(), reason = new Error("New connection");
  const pending = transport.call("work.list",null,controller.signal);
  controller.abort(reason); release("token");
  await assert.rejects(pending,(error)=>error===reason); assert.equal(requests,0);
});

test("Console base URL normalization preserves its pathname and configured query", async () => {
  const urls=[];
  const transport = new ConsoleClient("https://example.test/console?region=local#ignored",()=>"token",async(url)=>{urls.push(String(url));return Response.json(description);});
  await transport.manifest(); await transport.call("work.list");
  assert.deepEqual(urls,["https://example.test/console/manifest?region=local","https://example.test/console/actions/work.list?region=local"]);
});

test("malformed JSON is a protocol failure rather than an input or network failure", async () => {
  for (const status of [200,502]) {
    const transport = new ConsoleClient("https://example.test/console",()=>"token",async()=>new Response("<html>private upstream error</html>",{status}));
    await assert.rejects(transport.call("work.write",{}),(error)=>{
      assert.ok(error instanceof ActionError); assert.equal(error.code,"internal");
      assert.ok(error.cause instanceof SyntaxError); assert.ok(!error.message.includes("private")); return true;
    });
  }
});

for (const [name, invalid] of [
  ["duplicate pages",{...description,pages:[...description.pages,...description.pages]}],
  ["unknown references",{...description,pages:[{...description.pages[0],detail:"missing"}]}],
  ["duplicate modules",{...description,modules:[...description.modules,...description.modules]}],
  ["duplicate action ids",{...description,modules:[{...description.modules[0],actions:["work.list","work.list"]}]}],
]) {
  test(`external Console manifests reject ${name}`,async()=>{
    await assert.rejects(client(invalid).manifest(),{code:"internal"});
  });
}

test("cancellation while reading a response body prevents result parsing", async () => {
  let release, started;
  const ready = new Promise((resolve)=>{started=resolve;});
  const body = new Promise((resolve)=>{release=resolve;});
  const transport = new ConsoleClient("https://example.test/console",()=>"token",async()=>({ok:true,json(){started();return body;}}));
  const controller=new AbortController(), reason=new Error("Removed editor"); let parsed=0;
  const pending=transport.callParsed("work.list",{parse(value){parsed++;return value;}},null,controller.signal);
  await ready; controller.abort(reason); release({id:"late"});
  await assert.rejects(pending,(error)=>error===reason); assert.equal(parsed,0);
});
