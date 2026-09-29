#!/usr/bin/env python3
from pathlib import Path
import sys
p=Path(sys.argv[1])/'src/files.rs';s=p.read_text()
a=s.index('        let shell_c = CString::new("/bin/bash").unwrap();')
b=s.index('            libc::_exit(127);',a)
s=s[:a]+'''        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let shell_c = CString::new(shell.as_str()).unwrap_or_else(|_| CString::new("/bin/sh").unwrap());
        let arguments = if shell.ends_with("/bash") {
            vec![shell_c.clone(), CString::new("--norc").unwrap(), CString::new("--noprofile").unwrap()]
        } else {
            vec![shell_c.clone(), CString::new("-i").unwrap()]
        };
        let mut pointers: Vec<*const libc::c_char> = arguments.iter().map(|s| s.as_ptr()).collect();
        pointers.push(std::ptr::null());
        unsafe {
            libc::execvp(shell_c.as_ptr(), pointers.as_ptr());
'''+s[b:];p.write_text(s)
