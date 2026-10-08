import { mkdtemp, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
const root = resolve(import.meta.dir, '..');
const dir = await mkdtemp(join(tmpdir(), 'lenso-package-smoke-'));
const archives = join(dir, 'archives');
await mkdir(archives);
async function run(cmd: string[], cwd: string) {
  const proc = Bun.spawn(cmd, { cwd, stdout:'pipe',stderr:'pipe' });
  const [stdout,stderr,code] = await Promise.all([new Response(proc.stdout).text(),new Response(proc.stderr).text(),proc.exited]);
  if (code !== 0) throw new Error(`${cmd.join(' ')} failed (${code})\n${stdout}\n${stderr}`);
  return stdout;
}
const tarballs: Record<string,string> = {};
for (const name of ['lenso','cli','web']) {
  const pkg = join(root,'packages',name);
  await run([process.execPath,'pm','pack','--destination',archives],pkg);
}
for (const path of new Bun.Glob('*.tgz').scanSync({cwd:archives})) {
  const manifest = await run(['tar','-xOf',join(archives,path),'package/package.json'],dir);
  const pkg = JSON.parse(manifest);
  for (const range of Object.values(pkg.dependencies ?? {})) {
    if (typeof range === 'string' && range.includes('workspace:')) throw new Error(`Unconverted workspace dependency in ${pkg.name}`);
  }
  tarballs[pkg.name] = join(archives,path);
}
const core = join(dir,'core-only');
await mkdir(core);
await Bun.write(join(core,'package.json'),JSON.stringify({name:'lenso-core-consumer',type:'module',overrides:{lenso:tarballs.lenso},dependencies:{lenso:tarballs.lenso,'lenso-cli':tarballs['lenso-cli']}}));
await run([process.execPath,'install'],core);
await Bun.write(join(core,'lenso.config.ts'),`import {defineApp,definePlugin} from 'lenso';\nexport default defineApp({plugins:[definePlugin({id:'plain',setup:()=>({async greet({name}:{name:string}){return 'Hello, '+name}})})]});\n`);
const cli = await run([process.execPath,join(core,'node_modules/lenso-cli/dist/bin.js'),'call','plain','greet','{"name":"Consumer"}','--root',core],core);
if (!cli.includes('Hello, Consumer')) throw new Error('Packaged CLI failed');
for (const name of ['@orpc/server','@orpc/client','react','zod']) {
  if (await Bun.file(join(core,'node_modules',name,'package.json')).exists()) throw new Error(`Core consumer unexpectedly installed ${name}`);
}
const full = join(dir,'full');
await mkdir(full);
await Bun.write(join(full,'package.json'),JSON.stringify({name:'lenso-full-consumer',type:'module',overrides:tarballs,dependencies:{...tarballs,'@orpc/server':'1.15.5',zod:'4.6.5'},devDependencies:{typescript:'5.9.3','@types/bun':'1.4.2'}}));
await run([process.execPath,'install'],full);
await Bun.write(join(full,'smoke.ts'),`
import {definePlugin,startApp} from 'lenso';
import {createWebPlugin} from '@lenso/web';
import {createClient} from '@lenso/web/client';
import {os} from '@orpc/server';
import {z} from 'zod';
const business=definePlugin({id:'greeting',setup:()=>({async greet({name}:{name:string}){if(name.length<2)throw new Error('short');return {message:'Hello, '+name};}})});
const router=(service:{greet(input:{name:string}):Promise<{message:string}>})=>({greet:os.input(z.object({name:z.string()})).handler(({input})=>service.greet(input))});
const web=createWebPlugin({requires:[business],router:ctx=>router(ctx.get(business))});
const app=await startApp({plugins:[business,web]});
const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch:request=>app.get(web).fetch(request)});
try{
 const client=createClient<ReturnType<typeof router>>(new URL('/rpc',server.url));
 if(false){
 // @ts-expect-error numeric name must be rejected by actual inferred client
 await client.greet({name:123});
 }
 const result=await client.greet({name:'Consumer'});
 if(result.message!=='Hello, Consumer')throw new Error('typed HTTP mismatch');
 console.log(JSON.stringify({typedHTTP:result,plugins:app.status()}));
}finally{await server.stop(true);await app.stop();}
`);
await run([process.execPath,join(full,'node_modules/typescript/bin/tsc'),'--noEmit','--strict','--skipLibCheck','--target','ESNext','--module','ESNext','--moduleResolution','Bundler','smoke.ts'],full);
const fullResult=await run([process.execPath,join(full,'smoke.ts')],full);
await Bun.write(join(full,'browser.ts'),`import {createClient} from '@lenso/web/client'; console.log(createClient);`);
await run([process.execPath,'build','browser.ts','--target','browser','--outfile','browser.js'],full);
const browser=await Bun.file(join(full,'browser.js')).text();
if(browser.includes('Scope.make') || browser.includes('class RPCHandler') || browser.includes('node:fs'))throw new Error('Server leaked to browser bundle');
console.log(JSON.stringify({consumer:dir,coreCLI:JSON.parse(cli),coreHasNoWebDependencies:true,typecheck:true,full:JSON.parse(fullResult),browserBytes:new TextEncoder().encode(browser).byteLength},null,2));
