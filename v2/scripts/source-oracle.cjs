/* Execute byte-exact V1 TSX and locked @raycast/utils under a closed VM producer map.
   Run after prepare-v1.py + npm ci --ignore-scripts inside ignored qa/v1. */
const fs = require('node:fs'); const vm = require('node:vm'); const path = require('node:path');
const assert = require('node:assert/strict'); const crypto = require('node:crypto');
const root = path.resolve(__dirname, '..'); const v1 = path.join(root,'qa/v1');
process.env.HOME=path.join(root,'qa/home');process.env.TMPDIR=path.join(root,'qa/tmp');
const ts = require(path.join(v1,'node_modules/typescript')); const results=[];
function oracle({selected='https://example.test/a', clipboard='', prefs={}, response={link:'https://bit.ly/test'}, status=200, copyError=false}={}) {
  const events=[], effects=[], cache=new Map(), slots=[]; let cursor=0, pending=[], dirty=false, tree, search='', toasts=[];
  const preferences={accessToken:'synthetic-oracle-only',pasteAfterShortening:false,group_guid:'group_test',...prefs};
  const element=(type, props,...children)=>({type,props:{...props,...(children.length?{children:children.length===1?children[0]:children}:{})}});
  const changed=(a,b)=>!a||!b||a.length!==b.length||a.some((v,i)=>v!==b[i]);
  const react={createElement:element,Fragment:'Fragment',useRef(v){const i=cursor++;return slots[i]??(slots[i]={current:v});},useState(v){const i=cursor++;if(!(i in slots)){const slot={value:typeof v==='function'?v():v};slot.set=v=>{const next=typeof v==='function'?v(slot.value):v;if(next!==slot.value){slot.value=next;dirty=true;}};slots[i]=slot;}return [slots[i].value,slots[i].set];},useMemo(fn,deps){const i=cursor++;if(!slots[i]||changed(slots[i].deps,deps))slots[i]={deps,value:fn()};return slots[i].value;},useCallback(fn,deps){return this.useMemo(fn,deps);},useEffect(fn,deps){const i=cursor++;if(!slots[i]||changed(slots[i].deps,deps)){const old=slots[i];slots[i]={deps};pending.push(()=>{old?.cleanup?.();slots[i].cleanup=fn();});}},useSyncExternalStore(sub,get){const i=cursor++;if(!slots[i]){slots[i]={};slots[i].unsubscribe=sub(()=>{dirty=true;});}return get();}};
  react.useCallback=(fn,deps)=>react.useMemo(()=>fn,deps); react.default=react;
  const comp=name=>Object.assign(name==='List'?function List(){}:function Component(){},{displayName:name});
  const List=Object.assign(comp('List'),{Item:comp('List.Item'),EmptyView:comp('List.EmptyView')});
  const Action=Object.assign(comp('Action'),{CopyToClipboard:comp('Action.CopyToClipboard'),OpenInBrowser:comp('Action.OpenInBrowser')});
  const denied=()=>{throw new Error('BLOCKED producer');};
  class Cache {constructor(o){this.ns=o?.namespace??'';}get(k){return cache.get(this.ns+k);}set(k,v){cache.set(this.ns+k,v);dirty=true;}remove(k){cache.delete(this.ns+k);}subscribe(){return ()=>{};}clear(){cache.clear();}}
  const api={getPreferenceValues:()=>preferences,environment:{assetsPath:'/isolated/assets',supportPath:'/isolated/support',extensionName:'bitly-url-shortener',commandName:'list-links',launchType:'userInitiated'},LaunchType:{UserInitiated:'userInitiated'},Cache,
    Toast:{Style:{Animated:'animated',Success:'success',Failure:'failure'}},
    async showToast(...args){let v=typeof args[0]==='object'?{...args[0]}:{style:args[0],title:args[1]};events.push(['toast',v.title]);const proxy=new Proxy(v,{set(t,k,v){events.push(['toast-set',k,typeof v==='function'?'callback':typeof v==='object'?'action':v]);t[k]=v;return true;}});toasts.push(proxy);return proxy;},
    async getSelectedText(){events.push(['selection']);if(selected instanceof Error)throw selected;return selected;},
    Clipboard:{async readText(){events.push(['clipboard-read']);return clipboard;},async copy(text){events.push(['copy',text]);if(copyError)throw new Error('synthetic copy failure');},async paste(text){events.push(['paste',text]);}},
    openCommandPreferences(){events.push(['preferences']);},open:denied,LocalStorage:new Proxy({},{get:()=>denied}),List,Action,ActionPanel:comp('ActionPanel'),Icon:{},Color:{},MenuBarExtra:{},AI:{},OAuth:{},};
  const blockedModule=new Proxy({},{get:(_,k)=>k==='__esModule'?false:denied});
  const context=vm.createContext({URL,URLSearchParams,Response,Headers,Request,AbortController,AbortSignal,Buffer,TextEncoder,TextDecoder,structuredClone,console:{log:denied,warn:()=>{},error:()=>{}},setTimeout,clearTimeout,setInterval:denied,clearInterval:denied,process:{env:{HOME:path.join(root,'qa/home'),TMPDIR:path.join(root,'qa/tmp')},platform:'darwin'},fetch:async(url,options)=>{events.push(['fetch',String(url),options?.method??'GET',options?.body??null]);return new Response(JSON.stringify(response),{status,headers:{'Content-Type':'application/json'}});}});
  const modules=new Map();
  function load(file){
    if(modules.has(file))return modules.get(file).exports;
    const mod={exports:{}};modules.set(file,mod);
    let src=fs.readFileSync(file,'utf8');if(/\.tsx?$/.test(file))src=ts.transpileModule(src,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX,esModuleInterop:true}}).outputText;
    const req=(name)=>{
      if(name==='@raycast/api')return api;if(name==='react')return react;if(name==='react/jsx-runtime')return {jsx:(t,p)=>element(t,p),jsxs:(t,p)=>element(t,p)};
      if(name==='@raycast/utils')return load(path.join(v1,'node_modules/@raycast/utils/dist/main.js'));
      if(name==='dequal/lite')return load(path.join(v1,'node_modules/dequal/lite/index.js'));
      if(name==='node:path'||name==='path')return path;
      if(name==='node:url')return {URL};if(name==='node:crypto')return {createHash:crypto.createHash};
      if(['node:stream','node:util','node:buffer','node:string_decoder','node:events'].includes(name))return require(name);
      if(name.startsWith('node:'))return blockedModule;
      if(name.startsWith('./')){for(const ext of ['.ts','.tsx','.js']){const f=path.resolve(path.dirname(file),name+ext);if(fs.existsSync(f)&&f.startsWith(path.join(v1,'src')+path.sep))return load(f);}}
      throw new Error('BLOCKED module '+name);
    };
    const fn=new vm.Script('(function(require,module,exports){'+src+'\n})',{filename:file}).runInContext(context,{timeout:1000});fn(req,mod,mod.exports);return mod.exports;
  }
  function normalize(v){if(Object.prototype.toString.call(v)==='[object Date]')return v.toISOString();if(Array.isArray(v))return v.map(normalize);if(v&&typeof v==='object'){const o={};for(const[k,x]of Object.entries(v)){if(typeof x==='function')o[k]=x.displayName??'[callback]';else o[k]=normalize(x);}return o;}return typeof v==='function'?v.displayName??'[callback]':v;}
  return {events,api,async primary(){await toasts.at(-1)?.primaryAction?.onAction(toasts.at(-1));},async shorten(){await load(path.join(v1,'src/index.tsx')).default();return normalize(toasts);},async list(q=''){const fn=load(path.join(v1,'src/list-links.tsx')).default;function render(){cursor=0;tree=fn();for(const e of pending.splice(0))e();}render();if(q){tree.props.onSearchTextChange(q);dirty=true;}for(let i=0;i<30;i++){await new Promise(r=>setImmediate(r));if(dirty){dirty=false;render();}}return normalize(tree);},tree:()=>tree,normalize};
}
(async()=>{
  for(const [name,opts,expected]of [
    ['selected-literal',{selected:'  HTTPS://EXAMPLE.test/a/../b  '},'HTTPS://EXAMPLE.test/a/../b'],
    ['empty-fallback',{selected:' ',clipboard:' https://example.test/c '},'https://example.test/c'],
    ['rejected-fallback',{selected:new Error('none'),clipboard:'https://example.test/e'},'https://example.test/e'],
    ['custom-scheme',{selected:'javascript:alert(1)'},'javascript:alert(1)'],
    ['paste-order',{prefs:{pasteAfterShortening:true}},'https://example.test/a'],
    ['copy-failure',{copyError:true},'https://example.test/a'],
    ['invalid-input',{selected:'example.test'},null],
    ['empty-input',{selected:'',clipboard:''},null],
    ['auth-401',{status:401,response:{message:'INVALID_TOKEN'}},'https://example.test/a'],
    ['permission-403',{status:403,response:{description:'DENIED'}},'https://example.test/a'],
  ]){
    const o=oracle(opts);const toast=await o.shorten();const fetched=o.events.find(e=>e[0]==='fetch');assert.equal(fetched?JSON.parse(fetched[3]).long_url:null,expected);
    if(name==='paste-order')assert(o.events.findIndex(e=>e[0]==='copy')<o.events.findIndex(e=>e[0]==='paste'));
    if(name==='auth-401'){assert.equal(toast[0].title,'Invalid Access Token');await o.primary();assert(o.events.some(e=>e[0]==='preferences'));}if(name==='permission-403')assert.equal(toast[0].title,'Error');
    results.push({name,events:o.events,toast});
  }
  const row={id:'bit.ly/a',link:'https://bit.ly/a',title:'Original title',long_url:'https://example.test/a',created_at:'2026-01-02T03:04:05Z'};
  for(const [name,response,status,query]of [['list-success',{links:[row],pagination:{next:'https://evil.test/page2'}},200,''],['list-search',{links:[]},200,'a&b #'],['list-empty',{links:[]},200,''],['list-auth',{message:'INVALID_TOKEN'},401,'']]){
    const o=oracle({response,status});const tree=await o.list(query);assert(o.events.some(e=>e[0]==='fetch'));assert(o.events.filter(e=>e[0]==='fetch').length <= 2);assert(o.events.filter(e=>e[0]==='fetch').every(e=>!e[1].includes('evil.test')));
    if(name==='list-success'){const item=o.tree().props.children;const items=Array.isArray(item)?item:[item];if(!JSON.stringify(tree).includes('Original title'))fs.writeFileSync(path.join(root,'qa/oracle-debug.json'),JSON.stringify({tree,events:o.events},null,2));assert(JSON.stringify(tree).includes('Original title'));}
    if(name==='list-auth'){await o.primary();assert(o.events.some(e=>e[0]==='preferences'));}results.push({name,events:o.events,tree});
  }
  const urlCases=[];
  for(const input of ['', 'example.test', '/relative', 'http://', 'https://[oops', ' HTTPS://EXAMPLE.test/a/../b ', 'https:example.test', 'https://münich.test/π', 'mailto:hello@example.test', 'javascript:alert(1)', 'file:///tmp/example', 'ftp://example.test/a', '\uFEFFhttps://example.test/a\uFEFF', '\u0085https://example.test/a\u0085', 'https://example.test/a\nb', 'data:text/plain,hello', 'https://user:pass@example.test/a', 'http://127.0.0.1', 'http://0x7f000001', 'https://example.test/?a=%zz']) {
    const o=oracle({selected:input});await o.shorten();const f=o.events.find(e=>e[0]==='fetch');urlCases.push({input,accepted:!!f,literal:f?JSON.parse(f[3]).long_url:null});
  }
  fs.writeFileSync(path.join(root,'evidence/url-oracle.json'),JSON.stringify(urlCases,null,2)+'\n');
  fs.writeFileSync(path.join(root,'evidence/source-oracle.json'),JSON.stringify({source:'d946df7d3643a05f2bb3fadd12edf0b8a3858598',lockedUtils:'2.2.2',producerPolicy:'closed VM; fetch/selection/clipboard/preferences/cache injected; fs/os/process/network/browser denied',results},null,2)+'\n');
  console.log(`source oracle: ${results.length} executed cases passed`);
})().catch(e=>{console.error(e);process.exitCode=1;});
