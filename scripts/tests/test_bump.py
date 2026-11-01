import ast
import difflib
import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "bump.py"
FILES = ["VERSION", "Cargo.toml", "Cargo.lock", ".claude-plugin/plugin.json"]


@pytest.fixture
def repo(tmp_path):
    """A copy of the files bump.py edits, laid out as in the repository."""
    for name in FILES + ["scripts/bump.py"]:
        dest = tmp_path / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(REPO / name, dest)
    return tmp_path


def bump(repo, *args):
    return subprocess.run(
        [sys.executable, str(repo / "scripts" / "bump.py")] + list(args),
        capture_output=True,
        text=True,
        timeout=60,
    )


def changed_lines(old, new):
    diff = difflib.ndiff(old.splitlines(), new.splitlines())
    return [line for line in diff if line[:2] in ("- ", "+ ")]


def read_all(repo):
    """Text of every file with line endings normalized (Windows checkouts may use CRLF)."""
    return {name: (repo / name).read_bytes().decode("utf-8").replace("\r\n", "\n") for name in FILES}


def test_writes_every_version_and_nothing_else(repo):
    before = read_all(repo)
    r = bump(repo, "1.2.3")
    assert r.returncode == 0, r.stderr
    old = (REPO / "VERSION").read_text().strip()
    for name in FILES:
        assert "%s: %s -> 1.2.3" % (name, old) in r.stdout
    after = read_all(repo)
    assert after["VERSION"] == "1.2.3\n"
    assert json.loads(after[".claude-plugin/plugin.json"])["version"] == "1.2.3"
    assert '[workspace.package]\nversion = "1.2.3"\n' in after["Cargo.toml"]
    assert 'name = "tge"\nversion = "1.2.3"\n' in after["Cargo.lock"]
    for name in FILES[1:]:
        assert len(changed_lines(before[name], after[name])) == 2, name


def test_crlf_files_keep_their_line_endings(repo):
    for name in FILES[1:]:
        path = repo / name
        path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
    r = bump(repo, "1.2.3")
    assert r.returncode == 0, r.stderr
    for name in FILES[1:]:
        data = (repo / name).read_bytes()
        assert b"1.2.3" in data and data.count(b"\n") == data.count(b"\r\n"), name


def test_prerelease_is_accepted(repo):
    r = bump(repo, "0.1.0-rc.1")
    assert r.returncode == 0, r.stderr
    assert (repo / "VERSION").read_text() == "0.1.0-rc.1\n"


def test_same_version_changes_nothing(repo):
    current = (repo / "VERSION").read_text().strip()
    before = {name: (repo / name).read_bytes() for name in FILES}
    r = bump(repo, current)
    assert r.returncode == 0, r.stderr
    assert "unchanged" in r.stdout
    assert {name: (repo / name).read_bytes() for name in FILES} == before


@pytest.mark.parametrize("args", [[], ["1.2"], ["v1.2.3"], ["01.2.3"], ["1.2.3+build"], ["1.2.3", "x"]])
def test_bad_input_is_usage_error(repo, args):
    before = {name: (repo / name).read_bytes() for name in FILES}
    r = bump(repo, *args)
    assert r.returncode == 2
    assert r.stderr.startswith(("error:", "usage:"))
    assert {name: (repo / name).read_bytes() for name in FILES} == before


def test_missing_field_fails_without_writing(repo):
    lock = repo / "Cargo.lock"
    lock.write_text(lock.read_text().replace('name = "tge"', 'name = "other"'))
    before = {name: (repo / name).read_bytes() for name in FILES}
    r = bump(repo, "1.2.3")
    assert r.returncode == 1
    assert "Cargo.lock" in r.stderr
    assert {name: (repo / name).read_bytes() for name in FILES} == before


def test_python38_syntax():
    ast.parse(SCRIPT.read_text(encoding="utf-8"), feature_version=(3, 8))
