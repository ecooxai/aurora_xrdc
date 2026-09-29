#!/usr/bin/env python3
"""Fetch exactly the recorded sources; fail closed on checksum/revision mismatch."""
import argparse,hashlib,json,subprocess,urllib.request
from pathlib import Path
P=argparse.ArgumentParser();P.add_argument('destination',type=Path);args=P.parse_args();dest=args.destination.resolve();dest.mkdir(parents=True,exist_ok=True)
lock=json.loads((Path(__file__).parent/'sources.json').read_text())
urls={
 'dbus.tar.xz':'https://dbus.freedesktop.org/releases/dbus/dbus-1.16.2.tar.xz',
 'ffmpeg.tar.xz':'https://ffmpeg.org/releases/ffmpeg-7.1.3.tar.xz',
 'libXfont.tar.bz2':'https://www.x.org/releases/individual/lib/libXfont-1.5.4.tar.bz2',
 'libfontenc.tar.xz':'https://www.x.org/releases/individual/lib/libfontenc-1.1.8.tar.xz',
 'libsndfile.tar.xz':'https://github.com/libsndfile/libsndfile/releases/download/1.2.2/libsndfile-1.2.2.tar.xz',
 'opus.tar.gz':'https://downloads.xiph.org/releases/opus/opus-1.5.2.tar.gz',
 'libXau.tar.xz':'https://www.x.org/releases/individual/lib/libXau-1.0.11.tar.xz',
 'libXdmcp.tar.xz':'https://www.x.org/releases/individual/lib/libXdmcp-1.1.5.tar.xz',
 'libXtst.tar.xz':'https://www.x.org/releases/individual/lib/libXtst-1.2.5.tar.xz',
 'libXi.tar.xz':'https://www.x.org/releases/individual/lib/libXi-1.8.2.tar.xz',
 'libXinerama.tar.xz':'https://www.x.org/releases/individual/lib/libXinerama-1.1.5.tar.xz',
 'libvpx.tar.gz':'https://github.com/webmproject/libvpx/archive/refs/tags/v1.15.2.tar.gz',
 'x265.tar.gz':'https://bitbucket.org/multicoreware/x265_git/downloads/x265_4.1.tar.gz',
}
def run(argv,**kw):return subprocess.run(argv,check=True,**kw)
for name,record in lock['git'].items():
    path=dest/name
    if not path.exists():
        run(['git','init','-q',str(path)])
        run(['git','-C',str(path),'remote','add','origin',record['url']])
        run(['git','-C',str(path),'fetch','-q','--depth=1','origin',record['commit']])
        run(['git','-C',str(path),'checkout','-q','--detach','FETCH_HEAD'])
    revision=subprocess.check_output(['git','-C',str(path),'rev-parse','HEAD'],text=True).strip()
    if revision!=record['commit']:raise SystemExit(f'{name}: source revision mismatch; use a fresh build directory')
for filename,digest in lock['source_archives_sha256'].items():
    path=dest/filename
    if not path.exists():
        if filename=='libxkbcommon.tar.xz':
            data=subprocess.check_output(['git','-C',str(dest/'libxkbcommon'),'archive','--format=tar','--prefix=libxkbcommon-1.8.1/','HEAD'])
            with path.open('wb') as out:run(['xz','-c'],input=data,stdout=out)
        else:
            with urllib.request.urlopen(urls[filename],timeout=120) as source,path.open('wb') as output:
                while block:=source.read(1024*1024):output.write(block)
    if hashlib.sha256(path.read_bytes()).hexdigest()!=digest:raise SystemExit(f'{filename}: SHA256 mismatch')
    print(filename,'verified')
