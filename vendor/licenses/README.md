# Third-party components

The binaries are built from the pinned sources in `build/portable/sources.json`.
TinyX changes are GPL-3.0-or-later on its upstream X11/MIT base. PulseAudio and its static-loader patch are LGPL-2.1-or-later. D-Bus preserves its upstream dual licensing. FFmpeg, x264, x265, BusyBox and their corresponding-source/rebuild materials must retain upstream license terms. X11 libraries, libvpx, Opus, musl and libsndfile preserve their respective license notices.

Aurora WM and Aurora XRDC are the requested ecooxai projects. No top-level license file was present for Aurora WM at the pinned revision; do not infer a new redistribution license for that project. This is a private build for the requesting user, not a newly licensed public release. Embedded upstream font resources retain their original terms; no separate font files are distributed here.

Static linking does not make an upstream license disappear. Consult the included notices and source pins before redistributing.
