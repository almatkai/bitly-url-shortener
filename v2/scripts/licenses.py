#!/usr/bin/env python3
"""Collect exact available license/notice bytes for all Cargo-locked packages.
Missing files are recorded; SPDX metadata is not invented as a license text.
"""
import hashlib,json,pathlib,subprocess
R=pathlib.Path(__file__).resolve().parents[1]
meta=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--manifest-path',str(R/'Cargo.toml')]))
records=[]
for p in sorted(meta['packages'],key=lambda x:(x['name'],x['version'])):
 if p['name']=='tezbar-bitly-url-shortener':continue
 base=pathlib.Path(p['manifest_path']).parent;dest=R/'licenses/dependencies'/f"{p['name']}-{p['version']}";found=[]
 for f in sorted(base.rglob('*')):
  rel=f.relative_to(base)
  if len(rel.parts)>3 or not f.is_file() or f.is_symlink() or not any(part.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE','COPYRIGHT','AUTHORS')) for part in rel.parts):continue
  if f.stat().st_size>512*1024:continue
  b=f.read_bytes();out=dest/rel;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(b)
  found.append({'path':str(rel),'sha256':hashlib.sha256(b).hexdigest(),'bytes':len(b)})
 records.append({'name':p['name'],'version':p['version'],'license':p['license'],'source':p['source'],'repository':p['repository'],'files':found,'evidence': 'authentic distributed files' if found else 'No license text found in distributed package; metadata only; review required'})
(R/'licenses/dependency-index.json').write_text(json.dumps(records,indent=2)+'\n')
print('License texts:',len(records),'packages;',sum(not p['files'] for p in records),'missing-text records')
