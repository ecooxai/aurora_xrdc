#!/bin/sh
set -eu
export PKG_CONFIG_PATH=/out/lib/pkgconfig:/out/lib/pulseaudio/pkgconfig
export CFLAGS='-O2 -fPIC -ffunction-sections -fdata-sections'
export CXXFLAGS="$CFLAGS"
export LDFLAGS='-static -Wl,--gc-sections'
cd /build
for a in libXau libXdmcp libXtst libXi libXinerama; do tar xf "$a.tar.xz"; done
for src in libXau-1.0.11 libXdmcp-1.1.5 libXi-1.8.2 libXtst-1.2.5 libXinerama-1.1.5; do
  cd "/build/$src"
  ./configure --prefix=/out --disable-shared --enable-static --disable-devel-docs --without-xmlto --without-fop
  make -j4
  make install
done
cd /build
tar xf libxkbcommon.tar.xz
meson setup /build/xkb-build /build/libxkbcommon-1.8.1 --prefix=/out --default-library=static -Denable-x11=false -Denable-wayland=false -Denable-docs=false -Denable-tools=false -Denable-xkbregistry=false
ninja -C /build/xkb-build -j4
ninja -C /build/xkb-build install
cd /build/xdotool
make -j4 xdotool.static LDFLAGS='-static -Wl,--gc-sections' \
  CFLAGS="-O2 -I/out/include -I/usr/include -fPIC -std=gnu99" \
  LIBXDO_LIBS="$(pkgconf --static --libs xi x11 xtst xinerama xkbcommon)" \
  XDOTOOL_LIBS="$(pkgconf --static --libs x11)"
cp xdotool.static /out/bin/xdotool
cd /build
tar xf libvpx.tar.gz
cd libvpx-1.15.2
./configure --prefix=/out --disable-shared --enable-static --disable-examples --disable-tools --disable-docs --disable-unit-tests --enable-vp9-highbitdepth
make -j4
make install
cd /build
tar xf x265.tar.gz
python3 /build/patch-x265.py /build/x265_4.1
cmake -S x265_4.1/source -B /build/x265-build -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_INSTALL_PREFIX=/out -DENABLE_SHARED=OFF -DENABLE_CLI=OFF -DENABLE_LIBNUMA=OFF -DNATIVE_BUILD=OFF
cmake --build /build/x265-build -j4
cmake --install /build/x265-build
# Upstream libpulse.pc omits private static dependency closure.
sed -i 's|^Libs.private:.*|Libs.private: -L${libdir}/pulseaudio -lpulsecommon-17.0 -lsndfile -lm -pthread -lrt -ldl|' /out/lib/pkgconfig/libpulse.pc
cd /build/ffmpeg-7.1.3
./configure --prefix=/out --enable-gpl --enable-static --disable-shared --disable-doc --disable-debug --disable-ffplay --disable-autodetect \
  --enable-pthreads --enable-zlib --enable-libx264 --enable-libx265 --enable-libvpx --enable-libopus --enable-libpulse --enable-libxcb \
  --pkg-config-flags=--static --extra-cflags='-O2 -I/out/include' --extra-ldflags='-static -L/out/lib -Wl,--gc-sections' \
  --extra-libs='-lstdc++ -lm -pthread' --disable-network
make -j4
make install
for bin in ffmpeg ffprobe xdotool; do strip /out/bin/$bin; done
printf 'STATIC_MEDIA_READY\n'
