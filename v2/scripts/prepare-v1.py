#!/usr/bin/env python3
"""Create disposable, source-exact V1 QA in this clone. Does not install anything."""
import pathlib,subprocess
R=pathlib.Path(__file__).resolve().parents[2];Q=R/'v2/qa';source='d946df7d3643a05f2bb3fadd12edf0b8a3858598'
for d in ['home','tmp','npm-cache','v1','tooling']:(Q/d).mkdir(parents=True,exist_ok=True)
for p in subprocess.check_output(['git','ls-tree','-r','--name-only',source],cwd=R,text=True).splitlines():
 dest=Q/'v1'/p;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(subprocess.check_output(['git','show',source+':'+p],cwd=R))
for p in ['package.json','package-lock.json']:(Q/'tooling'/p).write_bytes((R/'v2/tooling'/p).read_bytes())
print('Prepared byte-exact V1 and pinned review-tool manifests in v2/qa only')
