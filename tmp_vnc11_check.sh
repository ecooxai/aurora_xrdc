#!/usr/bin/env bash
set -euo pipefail

pkill -f 'Xtightvnc :11' || true
rm -f /tmp/.X11-unix/X11 /tmp/.X11-lock

COOKIE="$(mcookie)"
xauth -f "$HOME/.Xauthority" remove "$(hostname)/unix:11" >/dev/null 2>&1 || true
xauth -f "$HOME/.Xauthority" add "$(hostname)/unix:11" . "$COOKIE"

nohup Xtightvnc :11 \
  -desktop X \
  -auth "$HOME/.Xauthority" \
  -geometry 1280x720 \
  -depth 24 \
  -rfbwait 120000 \
  -rfbauth "$HOME/.vnc/passwd" \
  -rfbport 5911 \
  -fp /usr/share/fonts/X11/misc/,/usr/share/fonts/X11/Type1/,/usr/share/fonts/X11/75dpi/,/usr/share/fonts/X11/100dpi/ \
  -co /etc/X11/rgb \
  >/tmp/xtightvnc11.log 2>&1 &
echo $! >/tmp/xtightvnc11.pid

sleep 2

DISPLAY=:11 XAUTHORITY="$HOME/.Xauthority" xdpyinfo | sed -n '1,20p'
DISPLAY=:11 XAUTHORITY="$HOME/.Xauthority" xsetroot -solid '#2e3440'

DISPLAY=:11 XAUTHORITY="$HOME/.Xauthority" \
  xterm -geometry 100x30+80+60 -title 'test xterm' >/tmp/xterm11.log 2>&1 &
echo $! >/tmp/xterm11.pid

sleep 2

echo '---PIDS---'
cat /tmp/xtightvnc11.pid /tmp/xterm11.pid
echo '---PS---'
ps -fp "$(cat /tmp/xtightvnc11.pid)" "$(cat /tmp/xterm11.pid)" || true
echo '---TREE---'
DISPLAY=:11 XAUTHORITY="$HOME/.Xauthority" xwininfo -root -tree | sed -n '1,220p'
echo '---SHOT---'
DISPLAY=:11 XAUTHORITY="$HOME/.Xauthority" \
  ffmpeg -y -loglevel error -f x11grab -video_size 1280x720 -i :11 -frames:v 1 /tmp/screen11_live.png
DISPLAY=:11 XAUTHORITY="$HOME/.Xauthority" \
  ffmpeg -y -loglevel error -f x11grab -video_size 1280x720 -i :11 -frames:v 1 /tmp/screen11_live.jpg

python3 - <<'PY'
from PIL import Image
for path in ['/tmp/screen11_live.png', '/tmp/screen11_live.jpg']:
    img = Image.open(path).convert('RGB')
    colors = sorted(img.getcolors(maxcolors=10_000_000), reverse=True)[:10]
    print(path, img.size, colors[:10])
PY
