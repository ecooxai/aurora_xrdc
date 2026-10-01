#!/bin/sh
# Build-only Alpine/musl environment. Never executed on the Debian runtime test target.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
WORK=${1:-"$ROOT/.agentwork/xdotool-build_gpt6_astra_pro_release_agent"}
mkdir -p "$WORK"; WORK=$(CDPATH= cd -- "$WORK" && pwd)
ALPINE=$WORK/alpine
[ ! -e "$ALPINE" ] || { echo 'Choose a fresh xdotool build directory' >&2; exit 1; }
python3 - "$ROOT" "$WORK" <<'PY'
import hashlib,json,subprocess,sys,urllib.request
from pathlib import Path
root,work=map(Path,sys.argv[1:]);lock=json.loads((root/'tools/portable/sources.json').read_text())
items={'libXau':'1.0.11','libXdmcp':'1.1.5','libXi':'1.8.2','libXtst':'1.2.5','libXinerama':'1.1.5'}
urls={'alpine.tar.gz':'https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/x86_64/alpine-minirootfs-3.24.2-x86_64.tar.gz'}
for n,v in items.items():urls[n+'.tar.xz']=f'https://www.x.org/releases/individual/lib/{n}-{v}.tar.xz'
for name,url in urls.items():
 path=work/name;expected=lock['alpine_rootfs_sha256'] if name=='alpine.tar.gz' else lock['source_archives_sha256'][name]
 if not path.exists():
  with urllib.request.urlopen(url,timeout=120) as response:path.write_bytes(response.read())
 assert hashlib.sha256(path.read_bytes()).hexdigest()==expected,name
for name in ['xdotool','libxkbcommon']:
 record=lock['git'][name];path=work/name
 subprocess.run(['git','init','-q',str(path)],check=True)
 subprocess.run(['git','-C',str(path),'fetch','-q','--depth=1',record['url'],record['commit']],check=True)
 subprocess.run(['git','-C',str(path),'checkout','-q','FETCH_HEAD'],check=True)
PY
mkdir "$ALPINE"
sudo tar -xzf "$WORK/alpine.tar.gz" -C "$ALPINE"
sudo cp /etc/resolv.conf "$ALPINE/etc/resolv.conf"
sudo mkdir -p "$ALPINE/etc/ssl/certs" "$ALPINE/build"
sudo cp /etc/ssl/certs/ca-certificates.crt "$ALPINE/etc/ssl/certs/"
for dev in null zero random urandom; do sudo cp -a "/dev/$dev" "$ALPINE/dev/"; done
sudo chmod 1777 "$ALPINE/tmp"
sudo chroot "$ALPINE" apk add --no-cache build-base bash python3 git curl autoconf automake libtool pkgconf meson ninja bison xorgproto libx11-dev libx11-static libxcb-dev libxcb-static libxau-dev libxdmcp-dev libxext-dev libxext-static libxtst-dev libxi-dev libxinerama-dev
sudo cp "$WORK/"*.tar.xz "$ALPINE/build/"
sudo cp -a "$WORK/xdotool" "$WORK/libxkbcommon" "$ALPINE/build/"
sudo cp "$ROOT/tools/portable/patch-xdotool.py" "$ALPINE/build/"
cat > "$WORK/build.sh" <<'BUILD'
#!/bin/sh
set -eu
export PKG_CONFIG_PATH=/out/lib/pkgconfig
cd /build
for f in *.tar.xz; do tar xf "$f"; done
for src in libXau-1.0.11 libXdmcp-1.1.5 libXi-1.8.2 libXtst-1.2.5 libXinerama-1.1.5; do
 cd "/build/$src"
 ./configure --prefix=/out --disable-shared --enable-static --disable-devel-docs --without-xmlto --without-fop
 make -j4; make install
done
meson setup /build/xkb-build /build/libxkbcommon --prefix=/out --default-library=static -Denable-x11=false -Denable-wayland=false -Denable-docs=false -Denable-tools=false -Denable-xkbregistry=false
ninja -C /build/xkb-build -j4
ninja -C /build/xkb-build install
cd /build/xdotool
python3 /build/patch-xdotool.py .
make -j4 xdotool.static LDFLAGS='-static -Wl,--gc-sections' CFLAGS='-O2 -I/out/include -fPIC -std=gnu99' LIBXDO_LIBS="$(pkgconf --static --libs xi x11 xtst xinerama xkbcommon)" XDOTOOL_LIBS="$(pkgconf --static --libs x11)"
strip xdotool.static
BUILD
sudo cp "$WORK/build.sh" "$ALPINE/build.sh"
sudo chroot "$ALPINE" sh /build.sh
cp "$ALPINE/build/xdotool/xdotool.static" "$ROOT/vendor/x86_64/bin/xdotool"
chmod 755 "$ROOT/vendor/x86_64/bin/xdotool"
sudo chroot "$ALPINE" apk info -v > "$ROOT/vendor/licenses/xdotool-build-packages.txt"
python3 - "$ROOT" <<'PY'
from pathlib import Path
import sys,importlib.util
r=Path(sys.argv[1]);spec=importlib.util.spec_from_file_location('audit',r/'tools/portable/package.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
print(m.audit_elf(r/'vendor/x86_64/bin/xdotool'))
PY
