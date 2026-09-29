#!/bin/sh
set -eu
export PKG_CONFIG_PATH=/out/lib/pkgconfig
export PKG_CONFIG=/build/pkg-config-static
printf '#!/bin/sh\nexec pkgconf --static "$@"\n' > "$PKG_CONFIG"
chmod +x "$PKG_CONFIG"
cd /build/tinyx
cp /build/tinyx-headless.c kdrive/fbdev/fbinit.c
python3 /build/patch-tinyx.py /build/tinyx
# GCC 14+ diagnoses old prototype conventions as errors; keep warnings visible.
export CFLAGS='-O2 -DINITARGS=void -std=gnu99 -fcommon -ffunction-sections -fdata-sections -Wno-error=implicit-function-declaration -Wno-error=incompatible-pointer-types -Wno-error=int-conversion'
export LDFLAGS='-static -Wl,--gc-sections'
autoreconf -fi
./configure --prefix=/out --disable-xvesa --enable-xfbdev --disable-xdmcp --disable-xdm-auth-1 --disable-ipv6 --with-default-font-path=built-ins
make -j4
rm -f kdrive/fbdev/Xfbdev
make -C kdrive/fbdev LDFLAGS="-all-static -Wl,--gc-sections" Xfbdev
mkdir -p /out/bin
cp kdrive/fbdev/Xfbdev /out/bin/Xtiny
strip /out/bin/Xtiny
/out/bin/Xtiny -version
