#!/usr/bin/env python3
"""Package and audit a relocatable static release. Requires only Python on the build host."""
import argparse,gzip,hashlib,json,os,shutil,struct,tarfile,subprocess,tomllib
from pathlib import Path

def audit_elf(path, machine=62):
    data=path.read_bytes()
    if data[:4]!=b'\x7fELF': raise ValueError(f'{path}: expected an ELF binary')
    if data[4:6]!=b'\x02\x01': raise ValueError(f'{path}: expected little-endian ELF64')
    if struct.unpack_from('<H',data,18)[0]!=machine: raise ValueError(f'{path}: wrong architecture')
    phoff=struct.unpack_from('<Q',data,32)[0]
    entsize,count=struct.unpack_from('<HH',data,54)
    if entsize<56 or phoff+entsize*count>len(data): raise ValueError(f'{path}: invalid ELF program headers')
    for i in range(count):
        kind,flags,offset,vaddr,paddr,size,mem,align=struct.unpack_from('<IIQQQQQQ',data,phoff+i*entsize)
        if kind==3: raise ValueError(f'{path}: PT_INTERP present (needs a runtime loader)')
        if kind==2:
            for d in range(offset,offset+size,16):
                tag,value=struct.unpack_from('<qQ',data,d)
                if tag==0: break
                if tag==1: raise ValueError(f'{path}: DT_NEEDED present (needs a shared library)')
    return {'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'elf':'ELF64-x86_64','interpreter':None,'needed':[]}

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--binaries',type=Path,required=True)
    ap.add_argument('--out',type=Path,default=Path('.output/dist'));ap.add_argument('--version',default=tomllib.loads((Path(__file__).resolve().parents[2]/'Cargo.toml').read_text())['package']['version']);ap.add_argument('--with-diagnostics',action='store_true',help='Include the optional static ffprobe test/diagnostic utility');args=ap.parse_args()
    root=Path(__file__).resolve().parents[2];out=args.out.resolve();out.mkdir(parents=True,exist_ok=True)
    name=f'aurora-xrdc-{args.version}-linux-x86_64';stage=out/name
    if stage.exists():shutil.rmtree(stage)
    (stage/'bin').mkdir(parents=True);(stage/'vendor/x86_64/bin').mkdir(parents=True)
    binaries={'aurora':args.binaries/'aurora','bin/vibe_rdesk':args.binaries/'vibe_rdesk'}
    required=['Xtiny','pulseaudio','pactl','pacat','dbus-daemon','dbus-send','ffmpeg','xdotool','aurora-wm','busybox']
    if args.with_diagnostics:required.append('ffprobe')
    for item in required:binaries[f'vendor/x86_64/bin/{item}']=root/'vendor/x86_64/bin'/item
    manifest={'name':name,'version':args.version,'architecture':'x86_64','libc':'musl (statically linked)','files':{},'runtime_packages':[]}
    try:manifest['source_revision']=subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True,stderr=subprocess.DEVNULL).strip()
    except (OSError,subprocess.CalledProcessError):manifest['source_revision']='unknown'
    for relative,source in binaries.items():
        info=audit_elf(source);dest=stage/relative;shutil.copyfile(source,dest);dest.chmod(0o755);manifest['files'][relative]=info
    # A small shell and common utilities make the private desktop usable even in a clean root.
    for applet in ['sh','ash','ls','cat','cp','mv','rm','mkdir','rmdir','pwd','echo','printf','env','id','whoami','uname','date','sleep','head','tail','grep','sed','awk','find','sort','wc','touch','chmod','ps','kill','df','du','free','tar','gzip','gunzip','vi','clear','which']:
        (stage/'vendor/x86_64/bin'/applet).symlink_to('busybox')
    for alias in ['paplay','parec','pamon','parecord']:(stage/'vendor/x86_64/bin'/alias).symlink_to('pacat')
    for source,dest in [('doc/portable.md','README.md'),('doc/portable-test-report.md','TEST-REPORT.md'),('run.sh','run.sh'),('vendor/licenses','licenses'),('tools/portable','build/portable')]:
        source=root/source
        if source.is_dir():shutil.copytree(source,stage/dest,ignore=shutil.ignore_patterns('__pycache__','*.pyc'))
        else:shutil.copy2(source,stage/dest)
    (stage/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    checks=[]
    for p in sorted(stage.rglob('*')):
        if p.is_file() and not p.is_symlink():checks.append(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(stage)}')
    (stage/'SHA256SUMS').write_text('\n'.join(checks)+'\n')
    epoch=int(os.environ.get('SOURCE_DATE_EPOCH','1790640000'))
    def normalize(info):
        info.uid=info.gid=0;info.uname=info.gname='';info.mtime=epoch
        if info.isdir():info.mode=0o755
        elif info.issym():info.mode=0o777
        else:info.mode=0o755 if info.name in [name+'/'+n for n in binaries] or info.name == name+'/run.sh' else 0o644
        return info
    archive=out/(name+'.tar.gz')
    with archive.open('wb') as f,gzip.GzipFile(filename='',mode='wb',fileobj=f,mtime=epoch,compresslevel=9) as gz,tarfile.open(fileobj=gz,mode='w',format=tarfile.PAX_FORMAT) as tar:
        tar.add(stage,arcname=name,filter=normalize)
    digest=hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix(archive.suffix+'.sha256').write_text(f'{digest}  {archive.name}\n')
    current=out/'current'
    if current.is_symlink():current.unlink()
    if not current.exists():current.symlink_to(name)
    print(json.dumps({'archive':str(archive),'bytes':archive.stat().st_size,'sha256':digest,'static_binaries':len(binaries)},indent=2))
if __name__=='__main__':main()
