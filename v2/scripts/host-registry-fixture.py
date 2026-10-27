#!/usr/bin/env python3
"""Generate narrow unapplied host patch and executable tests of extracted functions.
Host inputs are git-show snapshots at ea100cee, never current host sources.
"""
import difflib,pathlib,subprocess
R=pathlib.Path(__file__).resolve().parents[1];E=R/'evidence/host';Q=R/'qa/host-rust';Q.mkdir(exist_ok=True)
original=(E/'registry.rs').read_text();patched=original
patched=patched.replace('    path: PathBuf,\n    manifest: Value,','    path: PathBuf,\n    resource_root: PathBuf,\n    manifest: Value,')
patched=patched.replace('            let mut manifest = if native {','            let mut resource_root = path.clone();\n            let mut manifest = if native {')
patched=patched.replace('Ok(package) => package.manifest,','Ok(package) => { resource_root = package.dir; package.manifest },')
patched=patched.replace('                path,\n                manifest,','                path,\n                resource_root,\n                manifest,')
patched=patched.replace('        path,\n        manifest: Value::Null,','        resource_root: path.clone(),\n        path,\n        manifest: Value::Null,')
a=patched.index('    let native = if id.starts_with("raycast.") {',patched.index('fn installed_command'))
b=patched.index('    if let Some(extension) = native {',a)
patched=patched[:a]+'''    // collect resolves and validates active native packages before reading replacements.
    let native = collect(ctx).into_iter().find(|entry| {
        entry.id.starts_with("com.tezbar.native.")
            && (entry.id == id || entry.manifest["replaces"].as_array().is_some_and(|aliases|
                aliases.iter().any(|alias| alias.as_str() == Some(id))))
    });
'''+patched[b:]
marker='fn platform()'
helper='''pub(super) fn trusted_native_command_id(ctx: &CoreContext, id: &str, command: &str) -> Option<String> {
    installed_command(ctx, id, command)
        .filter(|entry| entry.id.starts_with("com.tezbar.native."))
        .map(|entry| entry.id)
}

'''
patched=patched.replace(marker,helper+marker)
# Only icon roots change. Canonical preference/activation/uninstall paths stay canonical.
for start,end in [('fn icon_path(', 'fn icon_data('),('fn command_icon_path(', 'fn command_icon_data(')]:
 a=patched.index(start);b=patched.index(end,a);part=patched[a:b].replace('extension.path.join(relative)','extension.resource_root.join(relative)').replace('extension.path.join("assets")','extension.resource_root.join("assets")');patched=patched[:a]+part+patched[b:]
native=(E/'native.rs').read_text();nativepatch=native.replace('|| v2_runtime::slug(requested).is_some()','|| v2_runtime::slug(requested).is_some()\n                || registry::trusted_native_command_id(ctx, requested, body["commandName"].as_str().unwrap_or("")).is_some()')
nativepatch=nativepatch.replace('v2_runtime::canonical_id(v2_runtime::slug(requested).ok_or("Unknown extension")?)','registry::trusted_native_command_id(ctx, requested, command)\n                .or_else(|| v2_runtime::slug(requested).map(v2_runtime::canonical_id))\n                .ok_or("Unknown extension")?')
listoriginal=(E/'list-runtime.tsx').read_text();listpatched=(R/'qa/host-patched/list-runtime.tsx').read_text()
patch=''
for name,old,new in [('src-tauri/src/native_core/extensions/registry.rs',original,patched),('src-tauri/src/native_core/extensions/native.rs',native,nativepatch),('src/renderer/src/raycast-api/list-runtime.tsx',listoriginal,listpatched)]:
 patch+=''.join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
(R/'integration-host.patch').write_text(patch)
# Compile original and patched actual blocks, with stubs ONLY at external core/installer boundaries.
imports='''use serde_json::{json,Value}; use std::{path::{Path,PathBuf,Component},collections::HashSet}; use base64::Engine;
use crate::{CoreContext,read_json};
'''
def extract(source):return imports+source[source.index('pub(super) struct Installed'):source.index('fn person(')]+source[source.index("fn fallback<'a>"):source.index('fn command_icon_data(')]
(Q/'original.rs').write_text(extract(original));(Q/'patched.rs').write_text(extract(patched))
(Q/'Cargo.toml').write_text('''[package]
name="bitly-host-fixture"
version="0.1.0"
edition="2021"
rust-version="1.85"
[dependencies]
serde_json="1"
base64="0.22"
tempfile="3"
''')
(Q/'src').mkdir(exist_ok=True)
(Q/'src/lib.rs').write_text('''#![allow(dead_code)]
use serde_json::Value; #[cfg(test)] use serde_json::json; use std::path::{Path,PathBuf};
struct Paths { data_dir:PathBuf } struct CoreContext { paths:Paths }
fn read_json(p:&Path)->Result<Value,String>{serde_json::from_slice(&std::fs::read(p).map_err(|_|"read")?).map_err(|_|"json".into())}
mod process_install {
 use super::*; pub struct Package{pub manifest:Value,pub dir:PathBuf}
 pub fn resolve(ctx:&CoreContext,id:&str)->Result<Package,String>{
 let root=ctx.paths.data_dir.join("extensions/packages").join(id);let active=read_json(&root.join("verified-fixture.json"))?;
 if active["verified"]!=true{return Err("Unverified".into())}let dir=root.join("versions/v1");Ok(Package{manifest:read_json(&dir.join("extension.json"))?,dir})
 }
}
mod v2_runtime {pub fn canonical_id(slug:&str)->String{format!("com.tezbar.native.{slug}")}}
mod original {include!("../original.rs");pub fn icon(ctx:&super::CoreContext,id:&str)->Option<String>{icon_path(&installed(ctx,id).ok()?)}}
mod patched {include!("../patched.rs");pub fn icon(ctx:&super::CoreContext,id:&str)->Option<String>{icon_path(&installed(ctx,id).ok()?)}}
#[test] fn active_icon_and_exact_verified_alias_coverage(){
 std::env::remove_var("SUPERCMD_EXTENSION_PATHS");
 let qa_tmp=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tmp");std::fs::create_dir_all(&qa_tmp).unwrap();
 let temp=tempfile::tempdir_in(qa_tmp).unwrap();let ctx=CoreContext{paths:Paths{data_dir:temp.path().into()}};let id="com.tezbar.native.bitly-url-shortener";
 let root=temp.path().join("extensions/packages").join(id);let version=root.join("versions/v1");std::fs::create_dir_all(version.join("resources")).unwrap();
 let mut m:Value=serde_json::from_str(include_str!("../../../extension.json")).unwrap();
 std::fs::write(root.join("verified-fixture.json"),r#"{"verified":true}"#).unwrap();
 std::fs::write(version.join("extension.json"),m.to_string()).unwrap();std::fs::write(version.join("resources/command-icon.png"),include_bytes!("../../../resources/command-icon.png")).unwrap();
 assert!(original::icon(&ctx,id).is_none());assert!(patched::icon(&ctx,id).unwrap().contains("versions/v1/resources/command-icon.png"));
 assert!(original::installed_command(&ctx,"com.tezbar.bitly-url-shortener","index").is_none());
 for alias in [id,"com.tezbar.bitly-url-shortener","raycast.bitly-url-shortener"]{for cmd in ["index","list-links"]{assert_eq!(patched::trusted_native_command_id(&ctx,alias,cmd).as_deref(),Some(id));}}
 for alias in ["com.tezbar.bitly","com.tezbar.bitly-url-shortener-extra","raycast.unrelated"]{assert!(patched::trusted_native_command_id(&ctx,alias,"index").is_none());}
 assert!(patched::trusted_native_command_id(&ctx,"com.tezbar.bitly-url-shortener","delete").is_none());
 m["replaces"]=json!([]);std::fs::write(version.join("extension.json"),m.to_string()).unwrap();assert!(patched::trusted_native_command_id(&ctx,"raycast.bitly-url-shortener","index").is_none());
 m["replaces"]=json!(["com.tezbar.bitly-url-shortener"]);m["commands"][0]["runtime"]=json!("javascript");std::fs::write(version.join("extension.json"),m.to_string()).unwrap();assert!(patched::trusted_native_command_id(&ctx,"com.tezbar.bitly-url-shortener","index").is_none());
 std::fs::write(root.join("verified-fixture.json"),r#"{"verified":false}"#).unwrap();assert!(patched::trusted_native_command_id(&ctx,id,"index").is_none());assert!(patched::icon(&ctx,id).is_none());
}
''')
subprocess.run(['cargo','test','--manifest-path',str(Q/'Cargo.toml')],check=True)
print('Extracted registry baseline regressions and patched verified alias/icon cases passed; separate unapplied patch generated')
