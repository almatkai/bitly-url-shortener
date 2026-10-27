const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');const mods=path.join(root,'qa/tooling/node_modules');
const Ajv=require(path.join(mods,'ajv/dist/2020'));const addFormats=require(path.join(mods,'ajv-formats'));
const schema=JSON.parse(fs.readFileSync(path.join(root,'evidence/host/extension-v3.schema.json')));
const ajv=new Ajv({strict:false});addFormats(ajv);const validate=ajv.compile(schema);
const manifest=JSON.parse(fs.readFileSync(path.join(root,'extension.json')));assert(validate(manifest),JSON.stringify(validate.errors));
for(const mutation of [m=>m.runtime='javascript',m=>m.protocolVersion=2,m=>m.execution='adapter',m=>m.capabilities.push('process.exec'),m=>m.replaces=['invented'],m=>m.artifacts={'aarch64-apple-darwin':{url:'http://bad',sha256:'fake'}},m=>m.commands[0].refreshMs=0,m=>m.unknown=true]){const m=structuredClone(manifest);mutation(m);assert(!validate(m));}
const selected=JSON.parse(fs.readFileSync(path.join(root,'evidence/host/loved_extensions.json')));
assert(JSON.stringify(selected).includes('raycast.bitly-url-shortener'));
const React=require(path.join(mods,'react'));const {renderToStaticMarkup}=require(path.join(mods,'react-dom/server'));
const ts=require(path.join(root,'qa/v1/node_modules/typescript'));
function loadList(source){
 const code=ts.transpileModule(source,{compilerOptions:{jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
 const mod={exports:{}};
 const deny=()=>{throw Error('Unexpected renderer producer');};
 const req=n=>{
  if(n==='react')return React;if(n==='react/jsx-runtime')return require(path.join(mods,'react/jsx-runtime'));
  if(n.endsWith('/primitives'))return {Hint:({children})=>children,HintBar:({children})=>children,Kbd:({children})=>children,ViewHeader:({title,children})=>React.createElement('header',{},title,children),cx:(...v)=>v.filter(Boolean).join(' ')};
  if(n.endsWith('/Markdown'))return {Markdown:deny};
  if(n==='./detail-runtime')return {MetadataItem:deny,MetadataSidebar:deny};
  if(n.endsWith('/colorConverter'))return {buildColorConversionResults:()=>[]};
  throw Error('Unexpected renderer import '+n);
 };
 vm.runInNewContext('(function(require,module,exports){'+code+'\n})',{console,window:new Proxy({},{get:deny})})(req,mod,mod.exports);return mod.exports.ListRuntime;
}
const source=fs.readFileSync(path.join(root,'evidence/host/list-runtime.tsx'),'utf8');
const emitted=JSON.parse(fs.readFileSync(path.join(root,'evidence/protocol-view.json')));
function html(component,view){return renderToStaticMarkup(React.createElement(component,{...view,onBack(){},onRunPrimaryAction(){},onOpenActions(){},onSearchTextChanged(){}}));}
const original=html(loadList(source),emitted);
assert(original.includes('&lt;img src=x&gt;'));assert(!original.includes('<img src=x>'));assert(!original.includes('href="https://evil.test'));assert(!original.includes('src="https://evil.test'));assert(!original.includes('<article'));
// Source list plain text stays inert through the actual host renderer. No Markdown patch needed.
const empty={...emitted,root:{type:'List',props:{searchText:'typed',__hasServerSearch:true},children:[{type:'List.EmptyView',props:{title:'No results found',description:'Try adjusting your search, filters, or try searching hidden links instead.'}}]}};
const baseline=html(loadList(source),empty);assert(baseline.includes('No matching applications')); // pinned host regression
const patched=source.replace(": hasQuery\n      ? 'No matching applications'",": emptyView?.title\n      ? emptyView.title\n      : hasQuery\n      ? 'No matching applications'").replace(": hasQuery\n      ? 'Try a different app name or keyword.'",": emptyView?.description\n      ? emptyView.description\n      : hasQuery\n      ? 'Try a different app name or keyword.'");
assert.notEqual(source,patched);const fixed=html(loadList(patched),empty);assert(fixed.includes('No results found'));assert(fixed.includes('Try adjusting your search'));
fs.mkdirSync(path.join(root,'qa/host-patched'),{recursive:true});fs.writeFileSync(path.join(root,'qa/host-patched/list-runtime.tsx'),patched);
console.log('Pinned schema validator + 8 negative cases; actual ListRuntime SSR inert remote content; empty-state regression and patch passed');
