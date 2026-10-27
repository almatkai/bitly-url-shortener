#!/usr/bin/env python3
"""Build a local dev-only ZIP; never publish/download/install. No invented artifact URL."""
import hashlib,json,pathlib,stat,zipfile
R=pathlib.Path(__file__).resolve().parents[1];D=R/'dist';D.mkdir(exist_ok=True)
binary=R/'target/aarch64-apple-darwin/release/bitly-url-shortener'
assert binary.is_file()
files={'bin/bitly-url-shortener':binary,'LICENSE':R/'LICENSE','NOTICE':R/'NOTICE'}
for parent in ['resources','licenses']:
 for p in (R/parent).rglob('*'):
  if p.is_file():assert not p.is_symlink();files[str(p.relative_to(R))]=p
sha=lambda b:hashlib.sha256(b).hexdigest()
manifest=json.loads((R/'extension.json').read_text());assert manifest['artifacts']=={}
manifest['files']={n:sha(p.read_bytes()) for n,p in sorted(files.items())}
archive=D/'bitly-url-shortener-aarch64-apple-darwin.zip'
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as z:
 for name,p in sorted(files.items()):
  info=zipfile.ZipInfo(name,date_time=(2026,1,1,0,0,0));info.create_system=3;info.external_attr=((stat.S_IFREG| (0o755 if name.startswith('bin/') else 0o644))<<16);z.writestr(info,p.read_bytes())
 z.writestr('extension.json',json.dumps(manifest,indent=2)+'\n')
extract=D/'extracted';extract.mkdir(exist_ok=True)
with zipfile.ZipFile(archive) as z:
 names=z.namelist();assert len(names)==len(set(names));assert set(names)==set(files)|{'extension.json'}
 z.extractall(extract)
 for name,digest in manifest['files'].items():assert sha((extract/name).read_bytes())==digest
(extract/'bin/bitly-url-shortener').chmod(0o755)
summary={'target':'aarch64-apple-darwin','developmentOnly':True,'archive':str(archive.relative_to(R)),'archiveBytes':archive.stat().st_size,'archiveSha256':sha(archive.read_bytes()),'executableSha256':manifest['files']['bin/bitly-url-shortener'],'executableBytes':binary.stat().st_size,'inventory':manifest['files']}
(R/'evidence/package-inventory.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k!='inventory'},indent=2))
