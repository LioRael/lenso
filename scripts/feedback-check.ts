import { createClient } from '../packages/web/dist/client';
import type { AppRouter } from '../examples/greeting/src/router';
const file = Bun.file(new URL('../examples/greeting/src/greeting.ts', import.meta.url));
const original = await file.text();
const api = createClient<AppRouter>('http://127.0.0.1:3000/rpc');
const listenerPid = async () => {
  const proc = Bun.spawn(['lsof','-nP','-iTCP:3000','-sTCP:LISTEN','-Fp'],{stdout:'pipe',stderr:'pipe'});
  const out = await new Response(proc.stdout).text();
  await proc.exited;
  const pids = out.split('\n').filter(line=>/^p\d+$/.test(line)).map(line=>Number(line.slice(1)));
  if(pids.length!==1)throw new Error(`Expected one listener, got ${out}`);
  return pids[0]!;
};
async function until(message:string) {
  const started=performance.now();
  while(performance.now()-started<10000){
    try { const result=await api.greet({name:'Feedback'}); if(result.message===message)return {result,elapsedMs:Math.round(performance.now()-started)}; } catch {}
    await Bun.sleep(100);
  }
  throw new Error(`New response not observed: ${message}`);
}
const before=await api.greet({name:'Feedback'});
const oldPid=await listenerPid();
try {
  await Bun.write(file,original.replace('Hello, ${trimmed}!','Welcome, ${trimmed}!'));
  const edited=await until('Welcome, Feedback!');
  const editedPid=await listenerPid();
  let oldGone=false; try{process.kill(oldPid,0);}catch{oldGone=true;}
  if(!oldGone || oldPid===editedPid)throw new Error('Old generation remained alive');
  await Bun.write(file,original);
  const restored=await until('Hello, Feedback!');
  const restoredPid=await listenerPid();
  console.log(JSON.stringify({before,oldPid,edited,editedPid,oldGone,restored,restoredPid,singleListener:true},null,2));
}finally{if(await file.text()!==original)await Bun.write(file,original);}
