#!/bin/sh
# Runs only inside the isolated Alpine musl builder. No host installation.
set -eu
export PKG_CONFIG_PATH=/out/lib/pkgconfig
export CFLAGS="-O2 -fPIC -ffunction-sections -fdata-sections"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="-static -Wl,--gc-sections"
cd /build
for a in *.tar.*; do tar xf "$a"; done
cmake -S libsndfile-1.2.2 -B sndfile-build -DCMAKE_INSTALL_PREFIX=/out -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DBUILD_SHARED_LIBS=OFF -DBUILD_PROGRAMS=OFF -DBUILD_EXAMPLES=OFF -DBUILD_TESTING=OFF -DENABLE_EXTERNAL_LIBS=OFF -DENABLE_MPEG=OFF
cmake --build sndfile-build -j4
cmake --install sndfile-build
cd /build/opus-1.5.2
./configure --prefix=/out --disable-shared --enable-static --disable-doc --disable-extra-programs
make -j4
make install
cd /build/libfontenc-1.1.8
./configure --prefix=/out --disable-shared --enable-static --disable-devel-docs
make -j4
make install
cd /build/libXfont-1.5.4
./configure --prefix=/out --disable-shared --enable-static --disable-devel-docs --disable-freetype --without-bzip2 --disable-fc --enable-builtins
make -j4
make install
cd /build/dbus-1.16.2
python3 /build/patch-dbus.py /build/dbus-1.16.2
meson setup /build/dbus-build --prefix=/out --default-library=static -Dprefer_static=true -Dc_link_args="$LDFLAGS" -Dapparmor=disabled -Dselinux=disabled -Dsystemd=disabled -Dx11_autolaunch=disabled -Dmodular_tests=disabled -Dauto_features=disabled -Dxml_docs=disabled -Ddoxygen_docs=disabled
ninja -C /build/dbus-build -j4
ninja -C /build/dbus-build install
cd /build/pulseaudio
python3 /build/patch-pulse.py /build/pulseaudio
meson setup /build/pulse-build --prefix=/out --default-library=static -Dprefer_static=true -Dc_link_args="$LDFLAGS" -Dauto_features=disabled -Ddatabase=simple -Ddaemon=true -Dclient=true -Dtests=disabled -Dman=false -Dlegacy-database-entry-format=false -Drunning-from-build-tree=false
ninja -C /build/pulse-build -j4
ninja -C /build/pulse-build install
printf 'STATIC_BASE_READY\n'
