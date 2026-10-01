#!/usr/bin/env python3
"""Preserve core-X11 input when the optional XKB extension is absent (TinyX).
Applies to upstream xdotool eb489de1b4fb3fd0cd935d68ae16ecd4c653ac7d.
"""
from pathlib import Path
import sys
p=Path(sys.argv[1])/'xdo.c';s=p.read_text()
if 'AURORA_CORE_X11_KEYMAP' in s:raise SystemExit('patch already applied; use an unmodified pinned source')
a=s.index('static void _xdo_populate_charcode_map(xdo_t *xdo) {')
b=s.index('\n/* context-free functions */',a)
body=s[a:b]
body=body.replace('  XFree(keysyms);\n','',1)
needle='''  XkbDescPtr desc = XkbGetMap(xdo->xdpy, XkbAllClientInfoMask, XkbUseCoreKbd);
'''
assert needle in body
body=body.replace(needle,needle+'''
  /* AURORA_CORE_X11_KEYMAP: TinyX has core keyboard mapping and XTEST, but
   * not XKB. Do not dereference a NULL extension map. Group 0's plain and
   * shifted symbols are sufficient for the core keyboard (no XKB groups). */
  if (desc == NULL) {
    if (keysyms == NULL || modmap == NULL || xdo->charcodes == NULL) {
      if (keysyms) XFree(keysyms);
      if (modmap) XFreeModifiermap(modmap);
      return;
    }
    for (keycode = xdo->keycode_low; keycode <= xdo->keycode_high; keycode++) {
      for (level = 0; level < xdo->keysyms_per_keycode && level < 2; level++) {
        KeySym keysym = keysyms[(keycode - xdo->keycode_low) * xdo->keysyms_per_keycode + level];
        if (keysym == NoSymbol) continue;
        xdo->charcodes[idx].key = _keysym_to_char(keysym);
        xdo->charcodes[idx].code = keycode;
        xdo->charcodes[idx].group = 0;
        xdo->charcodes[idx].modmask = (level ? ShiftMask : 0) |
          _xdo_query_keycode_to_modifier(modmap, keycode);
        xdo->charcodes[idx].symbol = keysym;
        idx++;
      }
    }
    xdo->charcodes_len = idx;
    XFree(keysyms);
    XFreeModifiermap(modmap);
    return;
  }
  XFree(keysyms);
''')
s=s[:a]+body+s[b:]
old='''    XkbStateRec state;
    XkbGetState(xdo->xdpy, XkbUseCoreKbd, &state);
    int current_group = state.group;
    XkbLockGroup(xdo->xdpy, XkbUseCoreKbd, key->group);'''
new='''    XkbStateRec state = {0};
    int has_xkb = XkbGetState(xdo->xdpy, XkbUseCoreKbd, &state) == Success;
    int current_group = has_xkb ? state.group : 0;
    if (has_xkb) XkbLockGroup(xdo->xdpy, XkbUseCoreKbd, key->group);'''
assert old in s;s=s.replace(old,new).replace('    XkbLockGroup(xdo->xdpy, XkbUseCoreKbd, current_group);','    if (has_xkb) XkbLockGroup(xdo->xdpy, XkbUseCoreKbd, current_group);')
p.write_text(s)
