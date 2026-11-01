"""Fail when a release binary needs a shared library beyond the system's own.

Usage: check_deps.py <binary>
Lists the dynamic dependencies with readelf -d (ELF), otool -L (Mach-O) or dumpbin /dependents
(PE) and fails on a C++ or compiler runtime (libstdc++, libc++.so, libgcc_s, vcruntime*, msvcp*).
ELF binaries may need only glibc's own libraries, Mach-O binaries only what ships with macOS
(/usr/lib, /System/Library). Standard library only.
"""
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

DENIED = re.compile(r"libstdc\+\+|libc\+\+\.so|libgcc_s|vcruntime|msvcp", re.I)
GLIBC = re.compile(r"^(?:(?:libc|libm|libpthread|libdl|librt|libutil|libresolv)\.so|ld-linux[-\w.]*\.so)\.\d+$")
MACOS_SYSTEM = ("/usr/lib/", "/System/Library/")
MAGIC = {
    b"\x7fELF": "elf",
    b"\xcf\xfa\xed\xfe": "macho",
    b"\xce\xfa\xed\xfe": "macho",
    b"\xca\xfe\xba\xbe": "macho",
}


def binary_kind(path):
    with open(path, "rb") as fh:
        head = fh.read(4)
    if head[:2] == b"MZ":
        return "pe"
    if head in MAGIC:
        return MAGIC[head]
    raise SystemExit("error: %s is not an ELF, Mach-O or PE binary" % path)


def parse_readelf(text):
    return re.findall(r"\(NEEDED\)[^\[\n]*\[([^\]\n]+)\]", text)


def parse_otool(text):
    return [line.split()[0] for line in text.splitlines()[1:] if line.strip() and not line.rstrip().endswith(":")]


def parse_dumpbin(text):
    return [line.strip() for line in text.splitlines() if re.match(r"^\s+\S+\.dll\s*$", line, re.I)]


def problems(kind, deps):
    out = []
    for dep in deps:
        if DENIED.search(dep):
            out.append("%s: a C++ or compiler runtime is linked dynamically" % dep)
        elif kind == "elf" and not GLIBC.match(dep):
            out.append("%s: not part of glibc" % dep)
        elif kind == "macho" and not dep.startswith(MACOS_SYSTEM):
            out.append("%s: does not ship with macOS" % dep)
    return out


def find_dumpbin():
    found = shutil.which("dumpbin")
    if found:
        return found
    program_files = os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")
    vswhere = Path(program_files) / "Microsoft Visual Studio" / "Installer" / "vswhere.exe"
    pattern = r"VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe"
    result = subprocess.run(
        [str(vswhere), "-latest", "-products", "*", "-find", pattern],
        capture_output=True,
        text=True,
        check=True,
    )
    paths = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if not paths:
        raise SystemExit("error: dumpbin.exe not found through vswhere")
    return paths[0]


def list_deps(kind, path):
    if kind == "elf":
        cmd, parse = ["readelf", "-d", path], parse_readelf
    elif kind == "macho":
        cmd, parse = ["otool", "-L", path], parse_otool
    else:
        cmd, parse = [find_dumpbin(), "/nologo", "/dependents", path], parse_dumpbin
    env = dict(os.environ, LC_ALL="C")
    result = subprocess.run(cmd, capture_output=True, text=True, check=True, env=env)
    return parse(result.stdout)


def main(argv):
    if len(argv) != 2:
        print("usage: check_deps.py <binary>", file=sys.stderr)
        return 2
    path = argv[1]
    kind = binary_kind(path)
    deps = list_deps(kind, path)
    print("%s (%s) needs:" % (path, kind))
    for dep in deps:
        print("  " + dep)
    if not deps:
        # Every target links the system C library dynamically, so an empty list means the tool's
        # output was not understood.
        print("error: no dependencies found; the listing was not parsed", file=sys.stderr)
        return 1
    found = problems(kind, deps)
    for problem in found:
        print("error: " + problem, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
