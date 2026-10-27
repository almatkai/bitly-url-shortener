#!/usr/bin/env python3
"""Whole Git-visible content inventory, excluding itself (INDEX.md only).
Does not stage files. Run last; index.py itself IS inventoried.
"""
import hashlib,pathlib,subprocess
R=pathlib.Path(__file__).resolve().parents[2]
files=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=R).decode().split('\0')
lines=['# Whole Git-visible review inventory','','SHA256 and byte size, including unchanged V1; excluding only v2/INDEX.md. Generated with `python3 v2/scripts/index.py`. Ignored QA, target, dist, ZIP and node_modules are excluded.','','```text']
for name in sorted(set(files)-{'','v2/INDEX.md'}):
 p=R/name
 if p.is_file():
  b=p.read_bytes();lines.append(f'{hashlib.sha256(b).hexdigest()} {len(b):>9} {name}')
lines.append('```');(R/'v2/INDEX.md').write_text('\n'.join(lines)+'\n')
print(len(lines)-6,'Git-visible files inventoried')
