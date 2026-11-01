# /// script
# requires-python = ">=3.8"
# dependencies = []
# ///
"""Set the release version everywhere it is recorded.

Writes VERSION, the [workspace.package] version in Cargo.toml (which crates/tge inherits), the
tge entry of Cargo.lock (so --locked builds keep working) and .claude-plugin/plugin.json.
Standard library only, Python 3.8+. Exit codes: 0 ok, 1 a file lacks its version, 2 usage.
"""
import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IDENT = r"(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-%s(?:\.%s)*)?$" % (IDENT, IDENT))

# File and the pattern of its version string (group 1); Cargo.toml is searched only inside its
# [workspace.package] table, plugin.json is checked afterwards to have changed its top-level key.
FIELDS = [
    ("Cargo.toml", re.compile(r'^version\s*=\s*"([^"]*)"', re.M)),
    ("Cargo.lock", re.compile(r'^name = "tge"\r?\nversion = "([^"]*)"', re.M)),
    (".claude-plugin/plugin.json", re.compile(r'"version"\s*:\s*"([^"]*)"')),
]


class BumpError(Exception):
    pass


def read(name):
    try:
        return (ROOT / name).read_bytes().decode("utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise BumpError("cannot read %s: %s" % (name, exc))


def search_range(name, text):
    """The (start, end) part of `text` holding the version."""
    if name != "Cargo.toml":
        return 0, len(text)
    table = re.search(r"^\[workspace\.package\][ \t]*\r?$", text, re.M)
    if not table:
        raise BumpError("Cargo.toml: no [workspace.package] table")
    following = re.compile(r"^\[", re.M).search(text, table.end())
    return table.end(), following.start() if following else len(text)


def plan(version):
    """[(name, old version, new text)] for every file; raises before anything is written."""
    text = read("VERSION")
    out = [("VERSION", text.strip(), version + "\n")]
    for name, pattern in FIELDS:
        text = read(name)
        start, end = search_range(name, text)
        match = pattern.search(text, start, end)
        if not match:
            raise BumpError("%s: version not found" % name)
        out.append((name, match.group(1), text[: match.start(1)] + version + text[match.end(1) :]))
    if json.loads(out[-1][2]).get("version") != version:
        raise BumpError(".claude-plugin/plugin.json: the first version key is not the top-level one")
    return out


def main(argv=None):
    parser = argparse.ArgumentParser(description="Set the release version (X.Y.Z or X.Y.Z-pre).")
    parser.add_argument("version")
    args = parser.parse_args(argv)
    if not SEMVER.match(args.version):
        print("error: %r is not a semantic version X.Y.Z[-pre]" % args.version, file=sys.stderr)
        return 2
    try:
        changes = plan(args.version)
    except BumpError as exc:
        print("error: %s" % exc, file=sys.stderr)
        return 1
    for name, old, new in changes:
        if old == args.version:
            print("%s: %s unchanged" % (name, old))
            continue
        (ROOT / name).write_bytes(new.encode("utf-8"))
        print("%s: %s -> %s" % (name, old, args.version))
    return 0


if __name__ == "__main__":
    sys.exit(main())
