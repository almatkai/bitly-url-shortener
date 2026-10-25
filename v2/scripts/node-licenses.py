#!/usr/bin/env python3
"""Record authentic distributed license files and explicit QA-only text gaps."""
import json,pathlib,hashlib
R=pathlib.Path(__file__).resolve().parents[1];records=[]
for scope in ['v1','tooling']:
 root=R/'qa'/scope;lock=json.loads((root/'package-lock.json').read_text())
 for name,p in lock['packages'].items():
  if not name:continue
  base=root/name
  if not base.exists():continue
  meta=json.loads((base/'package.json').read_text());dest=R/'licenses/qa-node'/scope/name.replace('node_modules/','');files=[]
  for f in base.iterdir():
   if f.is_file() and f.name.upper().startswith(('LICENSE','LICENCE','NOTICE','COPYING','COPYRIGHT','AUTHORS')) and f.stat().st_size<=524288:
    b=f.read_bytes();dest.mkdir(parents=True,exist_ok=True);(dest/f.name).write_bytes(b);files.append({'path':str((dest/f.name).relative_to(R)),'sha256':hashlib.sha256(b).hexdigest()})
  records.append({'scope':scope,'package':meta['name'],'version':meta['version'],'license':meta.get('license'),'integrity':p.get('integrity'),'files':files,'missingText':not bool(files)})
(R/'licenses/qa-dependency-index.json').write_text(json.dumps(records,indent=2)+'\n')
print(len(records),'QA dependencies;',sum(p['missingText'] for p in records),'explicit standalone license text gaps')
