#!/usr/bin/env python3
"""Allow a private machine-id file when /etc and /var/lib are absent/read-only."""
from pathlib import Path
import sys
p=Path(sys.argv[1])/'dbus/dbus-sysdeps-unix.c';s=p.read_text()
a=s.index('_dbus_read_local_machine_uuid (');b=s.index('\n}',a)+2
part=s[a:b]
part=part.replace('_dbus_string_init_const (&filename, DBUS_MACHINE_UUID_FILE);', '''_dbus_string_init_const (&filename,
      (_dbus_getenv ("AURORA_DBUS_MACHINE_ID_FILE") != NULL &&
       _dbus_getenv ("AURORA_DBUS_MACHINE_ID_FILE")[0] == '/')
      ? _dbus_getenv ("AURORA_DBUS_MACHINE_ID_FILE") : DBUS_MACHINE_UUID_FILE);''')
p.write_text(s[:a]+part+s[b:])
