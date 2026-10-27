#!/usr/bin/env python3
"""Fresh source snapshot and fresh Cargo home inside ignored QA; HTTPS SDK only.
No local SDK paths, Git overrides, app build or global config writes.
"""
import os,pathlib,shutil,subprocess,uuid
R=pathlib.Path(__file__).resolve().parents[1];Q=R/'qa'/('isolated-'+uuid.uuid4().hex[:8]);Q.mkdir()
for p in ['Cargo.toml','Cargo.lock']:shutil.copy2(R/p,Q/p)
for p in ['src','tests','evidence']:
 # Tests include URL oracle; no ignored outputs copied.
 shutil.copytree(R/p,Q/p)
manifest=(Q/'Cargo.toml').read_text();assert 'path = "../' not in manifest and 'git = "https://github.com/almatkai/tezbar-extension-sdk-rust.git"' in manifest and 'e955911f0bf5b558f9d3eb5183df0adc95c22af2' in manifest
for p in ['cargo-home','home','tmp']:(Q/p).mkdir()
env={k:v for k,v in os.environ.items() if k in ['PATH','RUSTUP_HOME','DEVELOPER_DIR','SDKROOT','LANG']}
env.update(HOME=str(Q/'home'),CARGO_HOME=str(Q/'cargo-home'),TMPDIR=str(Q/'tmp'),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='/dev/null',CARGO_NET_GIT_FETCH_WITH_CLI='false')
# rustup proxy needs existing toolchain; invoke resolved toolchain Cargo/Rustc explicitly.
cargo=subprocess.check_output(['rustup','which','cargo'],text=True).strip();rustc=subprocess.check_output(['rustup','which','rustc'],text=True).strip();env['RUSTC']=rustc
subprocess.run([cargo,'build','--locked','--release','--bin','bitly-url-shortener','--manifest-path',str(Q/'Cargo.toml')],env=env,check=True)
(R/'evidence/isolated-build.txt').write_text('PASS: fresh source copy, fresh Cargo home; immutable HTTPS SDK Git dependency, no local path/patch; production release binary built.\nQA directory: '+Q.name+'\n')
print('Isolated production release build passed:',Q.name)
