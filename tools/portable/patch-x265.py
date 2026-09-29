#!/usr/bin/env python3
from pathlib import Path
import sys
p=Path(sys.argv[1])/'source/CMakeLists.txt'
p.write_text(p.read_text().replace('CMP0025 OLD','CMP0025 NEW').replace('CMP0054 OLD','CMP0054 NEW'))
