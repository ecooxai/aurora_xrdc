#!/usr/bin/env python3
"""Static allowlisted PulseAudio: upstream native/null/remap modules, no dlopen."""
from pathlib import Path
import sys
root = Path(sys.argv[1])
modules = ["module-native-protocol-unix", "module-null-sink", "module-null-source", "module-remap-source", "module-remap-sink", "module-loopback", "module-sine-source", "module-always-sink"]
(root / ".tarball-version").write_text("17.0-aurora-g77d25a1\n")
for p in root.rglob("meson.build"):
    p.write_text(p.read_text().replace("shared_library(", "library(").replace("cdata.set('ENABLE_NLS', 1)", "cdata.set('ENABLE_NLS', false)"))
p = root / "meson.build"
p.write_text(p.read_text().replace("ltdl_dep = cc.find_library('ltdl', required : true)", "ltdl_dep = declare_dependency()"))
p = root / "src/meson.build"
p.write_text(p.read_text().replace("  subdir('daemon')\n  subdir('modules')", "  subdir('modules')\n  subdir('daemon')"))
p = root / "src/modules/meson.build"
s = p.read_text()
s = s[:s.index("foreach m : all_modules")] + "builtin_modules = []\nbuiltin_names = [" + ",".join(repr(m) for m in modules) + "]\n" + """
foreach m : all_modules
  name = m[0]
  if name not in builtin_names
    continue
  endif
  mod = static_library(name,
    m[1], m.get(2, []),
    include_directories: [configinc, topinc, include_directories('.')],
    c_args: [pa_c_args, server_c_args, '-DPA_MODULE_NAME=' + name.underscorify()] + m.get(3, []),
    dependencies: [thread_dep, libpulse_dep, libpulsecommon_dep, libpulsecore_dep, libintl_dep, platform_dep, platform_socket_dep] + m.get(4, []),
    link_with: m.get(5, []),
    install: false,
    implicit_include_directories: false)
  builtin_modules += mod
endforeach
"""
p.write_text(s)
p = root / "src/daemon/meson.build"
s = p.read_text().replace("  'ltdl-bind-now.c',", "  'ltdl-bind-now.c',\n  'static-modules.c',")
p.write_text(s.replace("  link_with : [libpulsecore],", "  link_with : [libpulsecore],\n  link_whole : builtin_modules,"))
(root / "src/ltdl.h").write_text("""/* SPDX-License-Identifier: LGPL-2.1-or-later */
#ifndef AURORA_STATIC_LTDL_H
#define AURORA_STATIC_LTDL_H
#include <stddef.h>
typedef void *lt_dlhandle;
typedef void *lt_ptr;
typedef struct { const char *name; void *address; } lt_dlsymlist;
#define LT_DLSYM_CONST const
extern const lt_dlsymlist lt_preloaded_symbols[];
#define LTDL_SET_PRELOADED_SYMBOLS() ((void)0)
lt_dlhandle lt_dlopenext(const char *name);
void *lt_dlsym(lt_dlhandle handle, const char *name);
int lt_dlclose(lt_dlhandle handle);
const char *lt_dlerror(void);
int lt_dlsetsearchpath(const char *path);
const char *lt_dlgetsearchpath(void);
int lt_dlforeachfile(const char *path, int (*callback)(const char *, void *), void *data);
#endif
""")
(root / "src/daemon/ltdl-bind-now.c").write_text("""/* SPDX-License-Identifier: LGPL-2.1-or-later */
#include "ltdl-bind-now.h"
void pa_ltdl_init(void) {}
void pa_ltdl_done(void) {}
""")
symbols = ["pa__init", "pa__done", "pa__get_n_used", "pa__get_author", "pa__get_description", "pa__get_usage", "pa__get_version", "pa__get_deprecated", "pa__load_once"]
code = "/* SPDX-License-Identifier: LGPL-2.1-or-later */\n#include <ltdl.h>\n#include <string.h>\n"
for mod in modules:
    name = mod.replace("-", "_")
    for sym in symbols:
        weak = "" if sym in ("pa__init", "pa__done", "pa__get_version") else " __attribute__((weak))"
        code += f"extern void {name}_LTX_{sym}(void){weak};\n"
code += "typedef struct { const char *name; void *symbols[9]; } builtin;\nstatic builtin table[] = {\n"
for mod in modules:
    name = mod.replace("-", "_")
    code += '{"' + mod + '", {' + ','.join('(void *)'+name+'_LTX_'+sym for sym in symbols) + '}},\n'
code += "};\nstatic const char *symbol_names[] = {" + ','.join('"'+s+'"' for s in symbols) + "};\n"
code += """
lt_dlhandle lt_dlopenext(const char *name) {
  for (size_t i=0; i<sizeof(table)/sizeof(table[0]); i++)
    if (!strcmp(name, table[i].name)) return &table[i];
  return NULL;
}
void *lt_dlsym(lt_dlhandle handle, const char *name) {
  if (!handle) return NULL;
  builtin *mod = handle;
  for (size_t i=0; i<9; i++) if (!strcmp(name, symbol_names[i])) return mod->symbols[i];
  return NULL;
}
int lt_dlclose(lt_dlhandle handle) { (void)handle; return 0; }
const char *lt_dlerror(void) { return "module not included in static fallback"; }
int lt_dlsetsearchpath(const char *path) { (void)path; return 0; }
const char *lt_dlgetsearchpath(void) { return NULL; }
int lt_dlforeachfile(const char *path, int (*callback)(const char *, void *), void *data) {
  (void)path;
  for (size_t i=0; i<sizeof(table)/sizeof(table[0]); i++) {
    int r=callback(table[i].name, data); if (r) return r;
  }
  return 0;
}
"""
(root / "src/daemon/static-modules.c").write_text(code)
p = root / "src/pulsecore/module.c"
s = p.read_text(); a = s.index("bool pa_module_exists("); b = s.index("\nvoid pa_module_hook_connect", a)
p.write_text(s[:a] + "bool pa_module_exists(const char *name) { return lt_dlopenext(name) != NULL; }\n" + s[b:])
print("Patched PulseAudio: eight built-in modules, static libraries, no dynamic loader")
