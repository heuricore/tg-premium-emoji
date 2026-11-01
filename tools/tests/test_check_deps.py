import importlib.util
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check_deps.py"
spec = importlib.util.spec_from_file_location("check_deps", SCRIPT)
DEPS = importlib.util.module_from_spec(spec)
spec.loader.exec_module(DEPS)

READELF = """
Dynamic section at offset 0x9c3d40 contains 30 entries:
  Tag        Type                         Name/Value
 0x0000000000000001 (NEEDED)             Shared library: [libm.so.6]
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]
 0x0000000000000001 (NEEDED)             Shared library: [ld-linux-x86-64.so.2]
 0x0000000000000001 (NEEDED)             Shared library: [libpthread.so.0]
 0x000000000000000c (INIT)               0x1d000
"""

OTOOL = """target/aarch64-apple-darwin/release/tge:
\t/usr/lib/libc++.1.dylib (compatibility version 1.0.0, current version 1700.255.5)
\t/usr/lib/libiconv.2.dylib (compatibility version 7.0.0, current version 7.0.0)
\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.100.2)
"""

DUMPBIN = """Dump of file target\\x86_64-pc-windows-msvc\\release\\tge.exe

File Type: EXECUTABLE IMAGE

  Image has the following dependencies:

    KERNEL32.dll
    ntdll.dll
    bcrypt.dll

  Summary

        1000 .data
"""


def test_parsers():
    assert DEPS.parse_readelf(READELF) == ["libm.so.6", "libc.so.6", "ld-linux-x86-64.so.2", "libpthread.so.0"]
    assert DEPS.parse_otool(OTOOL) == ["/usr/lib/libc++.1.dylib", "/usr/lib/libiconv.2.dylib", "/usr/lib/libSystem.B.dylib"]
    assert DEPS.parse_dumpbin(DUMPBIN) == ["KERNEL32.dll", "ntdll.dll", "bcrypt.dll"]


def test_system_libraries_pass():
    assert DEPS.problems("elf", DEPS.parse_readelf(READELF)) == []
    assert DEPS.problems("macho", DEPS.parse_otool(OTOOL)) == []
    assert DEPS.problems("pe", DEPS.parse_dumpbin(DUMPBIN)) == []


def test_runtimes_and_foreign_libraries_fail():
    assert len(DEPS.problems("elf", ["libstdc++.so.6", "libgcc_s.so.1", "libc++.so.1", "libssl.so.3"])) == 4
    assert len(DEPS.problems("macho", ["/opt/homebrew/lib/libfoo.dylib", "@rpath/libstdc++.6.dylib"])) == 2
    assert len(DEPS.problems("pe", ["VCRUNTIME140.dll", "MSVCP140.dll", "vcruntime140_1.dll"])) == 3


def test_kind_from_magic(tmp_path):
    for head, kind in [(b"\x7fELF", "elf"), (b"\xcf\xfa\xed\xfe", "macho"), (b"MZ\x90\x00", "pe")]:
        path = tmp_path / kind
        path.write_bytes(head + b"\x00" * 60)
        assert DEPS.binary_kind(str(path)) == kind
