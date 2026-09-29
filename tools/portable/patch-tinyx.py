#!/usr/bin/env python3
from pathlib import Path
import sys
root=Path(sys.argv[1])
p=root/'dix/dixfonts.c'
s=p.read_text(encoding="latin1").replace('#ifdef KDRIVESERVER\n        BuiltinRegisterFpeFunctions();\n#endif','        BuiltinRegisterFpeFunctions();')
p.write_text(s, encoding="latin1")
p=root/'kdrive/src/kinput.c'
p.write_text(p.read_text(encoding="latin1").replace('BYTE map[KD_MAX_BUTTON];','BYTE map[KD_MAX_BUTTON + 1];'), encoding="latin1")
