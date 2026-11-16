"""Desired R1-R4 application behavior, adapted from the read-only review.
Arguments: executable, existing shared synthetic source, NEW output directory.
Uses existing WCE_FB_SMOKE_GAME/CACHE, never foregrounds, every session quits.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys

exe, source, out = (Path(p).resolve() for p in sys.argv[1:4])
assert hashlib.sha256((source/'lf_height_map.dds').read_bytes()).hexdigest() == '0646a7af6c55d0ecc315c3bcfc20aa6e928c73aece05c75759daa248a2bb7cf4'
out.mkdir(parents=True, exist_ok=False)
settings=out/'settings'; settings.mkdir()
(settings/'settings.toml').write_text('game = "rome_2"\n[games.rome_2]\nmod_packs = []\n',encoding='utf-8')
env=dict(os.environ,WCE_SETTINGS_DIR=str(settings),WCE_PROJECTS=str(out/'global-projects'),
         WCE_GAME=os.environ['WCE_FB_SMOKE_GAME'],WCE_CACHE_DIR=os.environ['WCE_FB_SMOKE_CACHE'],
         WCE_ASSEMBLY_KIT=str(out/'no-kit'),WCE_DB_SCHEMAS=str(out/'no-schemas'),WCE_FOREGROUND='0')
name='fb_phase1_battle'; normal='normals/normal_0.dds'
golden='2ffa5f8f4bf4d8ed5dace39b67058b513c65a2d039f8b00c83ca278dbb32fe1e'
A='terrain_paint raise 15.75 16 1 9.155413138 1\nterrain_paint raise 16 16 1 9.155413138 1\n'
B='terrain_paint raise 16 16 1 4.577706569 1\nterrain_paint raise 16.25 16 1 4.577706569 1\n'
def q(p): return json.dumps(str(p).replace('\\','/'))
def create(root): return f'terrain_project_create {q(root)} {q(source)} {name}\nframes 2\n'
def opened(root): return f'terrain_project_open {q(root)} {name}\nframes 2\n'
def native(root): return root/'native'/name
def record(root): return root/'authoring/terrain/maps'/f'{name}.wce'
def inventory(root):
    return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file()}
runs=[]
def run(label,commands):
    script=out/(label+'.script');script.write_text('tool terrain\nframes 1\nassert_tool terrain\n'+commands+'quit\n',encoding='utf-8')
    log=out/(label+'.log')
    with log.open('wb') as stream:
        result=subprocess.run([str(exe),'--background','--script',str(script)],env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=90)
    text=log.read_text(errors='replace'); errors=[line for line in text.splitlines() if '[error]' in line]
    runs.append(dict(label=label,exit_code=result.returncode,errors=errors))
    (out/'progress.json').write_text(json.dumps(runs,indent=2),encoding='utf-8')
    print(json.dumps(runs[-1]),flush=True)
    assert result.returncode==0 and not errors,log
    return text
def block_save(): return '-save\n-terrain_save_answer overwrite\nassert_modified 1\n'
def repair(): return f'terrain_baseline_recover {q(source)}\n-save\nterrain_save_answer overwrite\nassert_modified 0\nclose\n'

# R1: a known generation block and real LF edits survive two fresh processes.
root=out/'generation-owner';slot=out/'generation-slot';again=out/'generation-again'
run('generation-seed',create(root)+A+'save\nclose\n')
path=native(root)/normal; damaged=bytearray(path.read_bytes());damaged[128]^=1;path.write_bytes(damaged)
before=inventory(native(root))
text=run('generation-blocked',opened(root)+B+f'terrain_recovery_copy {q(slot)}\nclose discard\n')
assert 'Native baseline blocked:' in text
text=run('generation-restored-blocked',f'terrain_recovery_open {q(slot)}\nframes 2\nassert_modified 1\n'+block_save()+f'terrain_recovery_copy {q(again)}\nclose discard\n')
assert 'Native baseline blocked:' in text and 'native height family differs' in text
assert inventory(native(root))==before
text=run('generation-repaired',f'terrain_recovery_open {q(again)}\nframes 2\nassert_modified 1\n'+repair())
assert hashlib.sha256((native(root)/normal).read_bytes()).hexdigest()==golden

# R3: the editable original-LF draft still opens when its pinned normal is gone.
root=out/'missing-owner';slot=out/'missing-slot'
run('missing-seed',create(root)+A+'save\nundo\nundo\nassert_modified 1\n'+f'terrain_recovery_copy {q(slot)}\nclose discard\n')
assert (slot/'map/lf_height_map.dds').is_file() and not (slot/'map'/normal).exists()
identity=record(root).read_text().splitlines()[2]
pinned=root/'authoring/terrain/baselines'/identity/normal
refs=dict(line.split('\t',1) for line in (slot/'refs.txt').read_text().splitlines())
assert Path(refs[normal]).resolve()==pinned.resolve(); pinned.unlink()
before=inventory(native(root))
text=run('missing-restored',f'terrain_recovery_open {q(slot)}\nframes 2\nassert_modified 1\n'+B+'undo\nundo\n'+block_save()+f'terrain_recovery_copy {q(out/"missing-again")}\nclose discard\n')
assert 'Native baseline blocked:' in text and inventory(native(root))==before
run('missing-repaired',f'terrain_recovery_open {q(out/"missing-again")}\nframes 2\nassert_modified 1\n'+repair())
assert (native(root)/'lf_height_map.dds').read_bytes()==(source/'lf_height_map.dds').read_bytes()
assert (native(root)/normal).read_bytes()==(source/normal).read_bytes()

# R4: rejection is BEFORE any native inventory or destination mutation.
root=out/'nested-owner';run('nested-seed',create(root)+'close\n')
target=native(root)/'nested-owner';before=inventory(root)
text=run('nested-rejected',opened(root)+A+f'-terrain_project_copy {q(target)}\nassert_modified 1\nclose discard\n')
assert 'overlap' in text.lower() and inventory(root)==before and not target.exists()

# R2: materialize both review publication states, then use the real loader/Save.
for mode in ['first-publication','truncated-completion']:
    root=out/mode;run(mode+'-seed',create(root)+A+'save\nclose\n')
    prefix,generation=record(root).read_text().split('--generation--\n')
    if mode=='first-publication':
        record(root).write_text(prefix+'--generation--\n',encoding='utf-8')
        for key in ['lf_height_map.dds','lf_height_map.data']:
            shutil.copyfile(source/key,native(root)/key)
    else:
        tail='\n'.join(line for line in generation.splitlines() if not line.startswith(normal))+'\n'
        record(root).write_text(prefix+'--generation--\n'+tail,encoding='utf-8')
        path=native(root)/normal;value=bytearray(path.read_bytes());value[128]^=1;path.write_bytes(value)
    before=inventory(native(root))
    text=run(mode+'-blocked',opened(root)+B+block_save()+'close discard\n')
    assert 'Native baseline blocked:' in text and inventory(native(root))==before

receipt=dict(processes=len(runs),runs=runs,known_block_preserved=True,missing_original_editable=True,
             generation_inventories_fail_closed=True,rejected_copy_read_only=True,verified_repairs_succeed=True,
             normal_reference_sha256=golden,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),foreground=False)
(out/'receipt.json').write_text(json.dumps(receipt,indent=2),encoding='utf-8');print(json.dumps(receipt),flush=True)
