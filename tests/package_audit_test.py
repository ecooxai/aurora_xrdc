import importlib.util,struct,tempfile,unittest
from pathlib import Path
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('package',root/'tools/portable/package.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class ElfAuditTest(unittest.TestCase):
    def elf(self,kind=1):
        data=bytearray(128);data[:6]=b'\x7fELF\x02\x01';struct.pack_into('<H',data,18,62);struct.pack_into('<Q',data,32,64);struct.pack_into('<HH',data,54,56,1)
        struct.pack_into('<IIQQQQQQ',data,64,kind,0,120,0,0,0,0,0);return data
    def audit(self,data):
        with tempfile.NamedTemporaryFile() as f:
            f.write(data);f.flush();return module.audit_elf(Path(f.name))
    def test_static_elf_passes(self):self.assertEqual(self.audit(self.elf())['needed'],[])
    def test_dynamic_loader_rejected(self):
        with self.assertRaisesRegex(ValueError,'PT_INTERP'):self.audit(self.elf(3))
    def test_shared_library_rejected(self):
        data=self.elf(2);data.extend(b'\0'*16);struct.pack_into('<Q',data,64+32,16);struct.pack_into('<qQ',data,120,1,0)
        with self.assertRaisesRegex(ValueError,'DT_NEEDED'):self.audit(data)
    def test_wrong_architecture_rejected(self):
        data=self.elf();struct.pack_into('<H',data,18,183)
        with self.assertRaisesRegex(ValueError,'architecture'):self.audit(data)
if __name__=='__main__':unittest.main()
