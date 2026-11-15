"""Bounded asset-free WCE application smoke; all destinations must be new.
Arguments: WCE executable, existing Phase 1 synthetic source, new output root.
Never foregrounds a window or touches an existing WCE process.
"""
from pathlib import Path
import hashlib, json, os, struct, subprocess, sys, time

exe, source, out = map(lambda p: Path(p).resolve(), sys.argv[1:4])
assert hashlib.sha256((source/'lf_height_map.dds').read_bytes()).hexdigest() == '0646a7af6c55d0ecc315c3bcfc20aa6e928c73aece05c75759daa248a2bb7cf4'
assert hashlib.sha256((source/'normals/normal_0.dds').read_bytes()).hexdigest() == '21811002f13d2ab5e540d38bb483fc420956ea92f993cfcbdd4cd2760062c7c6'
out.mkdir(parents=True, exist_ok=False)
settings=out/'settings'; settings.mkdir()
(settings/'settings.toml').write_text('game = "rome_2"\n[games.rome_2]\nmod_packs = []\n', encoding='utf-8')
game=Path(os.environ.get('WCE_FB_SMOKE_GAME',os.environ.get('WCE_GAME',''))).resolve()
assert (game/'Rome2.exe').is_file(), 'set WCE_FB_SMOKE_GAME to the existing Rome II installation' 
env=dict(os.environ, WCE_SETTINGS_DIR=str(settings),WCE_CACHE_DIR=os.environ.get('WCE_FB_SMOKE_CACHE',str(out/'cache')),WCE_PROJECTS=str(out/'global-projects'),WCE_GAME=os.environ.get('WCE_FB_SMOKE_GAME',str(game)),WCE_ASSEMBLY_KIT=str(out/'no-kit'),WCE_DB_SCHEMAS=str(out/'no-schemas'),WCE_FOREGROUND='0')
name='fb_phase1_battle'
def q(p):return json.dumps(str(p).replace('\\','/'))
A='terrain_paint raise 15.75 16 1 9.155413138 1\nterrain_paint raise 16 16 1 9.155413138 1\n'
B='terrain_paint raise 16 16 1 4.577706569 1\nterrain_paint raise 16.25 16 1 4.577706569 1\n'
records=[]
def run(label,commands):
    script=out/(label+'.script');script.write_text('tool terrain\nframes 1\nassert_tool terrain\n'+commands+'quit\n',encoding='utf-8')
    start=time.monotonic();log=out/(label+'.log')
    with log.open('wb') as stream:
        result=subprocess.run([str(exe),'--background','--script',str(script)],env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=90)
    errors=[line for line in log.read_text(errors='replace').splitlines() if '[error]' in line or 'script failed' in line.lower() or 'start failed' in line.lower()]
    expected_prompts=0
    if label=='autosave-restore':
        contents=log.read_text(errors='replace')
        assert 'terrain_save_answer cancel' in contents and 'terrain_save_answer overwrite' in contents
        expected_prompts=2 # Both answers fail unless the real Save prompt exists.
    records.append(dict(label=label,exit_code=result.returncode,seconds=round(time.monotonic()-start,3),errors=errors,expected_save_prompts=expected_prompts))
    (out/'result.json').write_text(json.dumps(records,indent=2),encoding='utf-8')
    print(json.dumps(records[-1]),flush=True)
    assert result.returncode==0 and not errors,log
def create(root):return f'terrain_project_create {q(root)} {q(source)} {name}\nframes 2\n'
def open_project(root):return f'terrain_project_open {q(root)} {name}\nframes 2\n'
continuous=out/'continuous';reopened=out/'reopened';kept=out/'kept-open';aware=out/'aware-copy';standalone=out/'standalone';exported=out/'export'
run('continuous',create(continuous)+A+B+'assert_modified 1\nundo\nredo\n'+f'terrain_save_copy {q(standalone)}\nassert_modified 1\nterrain_project_copy {q(aware)}\nassert_modified 1\nsave\nassert_modified 0\nterrain_export {q(exported)} --wait\nclose\n'+open_project(continuous)+'assert_modified 0\nsave\nclose\n')
run('reopened-A',create(reopened)+A+'save\nassert_modified 0\nclose\n')
run('reopened-B',open_project(reopened)+B+'save\nassert_modified 0\nclose\n')
run('kept-open',create(kept)+A+'save\n'+B+'save\nassert_modified 0\nclose\n')
def files(folder):return {p.relative_to(folder).as_posix():p.read_bytes() for p in folder.rglob('*') if p.is_file()}
expected=files(continuous/'native'/name)
for p in [reopened/'native'/name,kept/'native'/name,aware/'native'/name,standalone,exported/'terrain/battles'/name]:assert files(p)==expected,p
assert expected['normals/normal_0.dds']!=(source/'normals/normal_0.dds').read_bytes()
assert struct.unpack_from('<I',expected['normals/normal_0.dds'],28)[0]==8
assert hashlib.sha256(expected['normals/normal_0.dds']).hexdigest()=='2ffa5f8f4bf4d8ed5dace39b67058b513c65a2d039f8b00c83ca278dbb32fe1e'
rows=struct.unpack_from('<'+'H'*(192*192),expected['lf_height_map.dds'],128)
assert (rows[64*192+63],rows[64*192+64],rows[64*192+65])==(10200,10300,10100)
assert not any('authoring' in k for k in expected)
assert expected['opaque.bin']==(source/'opaque.bin').read_bytes()
relocated=out/'relocated';aware.rename(relocated)
run('relocated',open_project(relocated)+'save\nassert_modified 0\nclose\n')
assert files(relocated/'native'/name)==expected
recovery=out/'recovery-owner'
run('autosave-create',create(recovery)+A+'autosave_now\nautosave_status\nautosave_abandon\n')
recovered=out/'recovered-copy'
run('autosave-restore','autosave_found\nautosave_restore 0\nframes 4\nassert_modified 1\n'+B+f'terrain_project_copy {q(recovered)}\nassert_modified 1\n-save\nterrain_save_answer cancel\nassert_modified 1\n-save\nterrain_save_answer overwrite\nassert_modified 0\nclose\n'+open_project(recovered)+'save\nassert_modified 0\nclose\n')
assert files(recovered/'native'/name)==expected
assert files(recovery/'native'/name)==expected
# Exercise the actual loader's blocked state and verified recovery, preserving
# the golden final LF. Delete only one generated synthetic baseline resource.
record=(continuous/'authoring/terrain/maps'/f'{name}.wce').read_text()
baseline_id=record.splitlines()[2]
missing=(continuous/'authoring/terrain/baselines'/baseline_id/'normals/normal_0.dds').resolve()
assert missing.is_relative_to(out) and missing.is_file()
missing.unlink()
run('blocked-recovery',open_project(continuous)+f'terrain_recovery_copy {q(out/"blocked-slot")}\n-save\nterrain_baseline_recover {q(source)}\nsave\nassert_modified 0\nclose\n')
assert files(continuous/'native'/name)==expected
costs=[]
for root in [continuous,reopened,recovery]:
    store=root/'authoring';payload=sum(p.stat().st_size for p in (store/'terrain/baselines').rglob('*') if p.is_file());record=sum(p.stat().st_size for p in (store/'terrain/maps').rglob('*') if p.is_file());costs.append(dict(project=root.name,payload_bytes=payload,record_bytes=record,total_bytes=payload+record))
receipt=dict(normal_sha256=hashlib.sha256(expected['normals/normal_0.dds']).hexdigest(),native={k:dict(bytes=len(v),sha256=hashlib.sha256(v).hexdigest()) for k,v in expected.items()},runs=records,costs=costs,foreground=False,synthetic=True,game_runtime_certification=False,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest())
(out/'receipt.json').write_text(json.dumps(receipt,indent=2),encoding='utf-8');print(json.dumps(receipt),flush=True)
