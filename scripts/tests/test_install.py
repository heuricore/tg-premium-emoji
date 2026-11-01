import ast
import hashlib
import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import threading
import zipfile
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "install.py"


def load_module():
    spec = importlib.util.spec_from_file_location("tge_install", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


INSTALL = load_module()
PLATFORM = INSTALL.platform_id()
IS_WINDOWS = PLATFORM.startswith("windows")
BIN_NAME = "tge.exe" if IS_WINDOWS else "tge"


def build_archive(version, members=None):
    """Return (archive_name, bytes) for a release archive."""
    top = "tge-v%s-%s" % (version, PLATFORM)
    if members is None:
        members = {
            top + "/" + BIN_NAME: b"binary " + version.encode(),
            top + "/catalog/pack.json": b"{}",
        }
    buf = io.BytesIO()
    if IS_WINDOWS:
        name = top + ".zip"
        with zipfile.ZipFile(buf, "w") as zf:
            for path, data in members.items():
                zf.writestr(path, data)
    else:
        name = top + ".tar.gz"
        with tarfile.open(fileobj=buf, mode="w:gz") as tf:
            for path, data in members.items():
                info = tarfile.TarInfo(path)
                info.size = len(data)
                info.mode = 0o644
                tf.addfile(info, io.BytesIO(data))
    return name, buf.getvalue()


class Server:
    def __init__(self):
        self.files = {}
        self.releases = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                if self.path == "/releases":
                    body = json.dumps(outer.releases).encode()
                elif self.path in outer.files:
                    body = outer.files[self.path]
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        self.httpd = HTTPServer(("127.0.0.1", 0), Handler)
        self.base = "http://127.0.0.1:%d" % self.httpd.server_port
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    @property
    def api(self):
        return self.base + "/releases"

    def add_release(self, version, prerelease=False, draft=False, archive=None, checksum=None):
        tag = "v" + version
        name, data = archive if archive else build_archive(version)
        digest = checksum or hashlib.sha256(data).hexdigest()
        self.files["/" + name] = data
        self.files["/" + name + ".sha256"] = ("%s  %s\n" % (digest, name)).encode()
        self.releases.append(
            {
                "tag_name": tag,
                "draft": draft,
                "prerelease": prerelease,
                "assets": [
                    {"name": name, "browser_download_url": self.base + "/" + name},
                    {
                        "name": name + ".sha256",
                        "browser_download_url": self.base + "/" + name + ".sha256",
                    },
                ],
            }
        )

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()


@pytest.fixture
def server():
    s = Server()
    yield s
    s.close()


@pytest.fixture
def env(tmp_path, server):
    """Plugin layout copy (scripts/install.py + VERSION) and a state home."""
    plugin = tmp_path / "plugin"
    (plugin / "scripts").mkdir(parents=True)
    shutil.copy(SCRIPT, plugin / "scripts" / "install.py")
    (plugin / "VERSION").write_text("0.0.1\n")
    home = tmp_path / "home"
    e = dict(os.environ)
    e["TG_PREMIUM_EMOJI_HOME"] = str(home)
    e["TGE_RELEASES_API"] = server.api
    e["PYTHONDONTWRITEBYTECODE"] = "1"
    return {"plugin": plugin, "home": home, "env": e}


def run(env, *args):
    return subprocess.run(
        [sys.executable, str(env["plugin"] / "scripts" / "install.py")] + list(args),
        env=env["env"],
        capture_output=True,
        text=True,
        timeout=60,
    )


def latest_version(env):
    latest = env["home"] / "bin" / "latest"
    return os.path.basename(os.path.realpath(str(latest)))


def snapshot(home):
    bin_dir = home / "bin"
    if not bin_dir.exists():
        return None
    return sorted(
        (str(p.relative_to(bin_dir)), os.path.realpath(str(p)) if p.is_symlink() else "")
        for p in bin_dir.rglob("*")
    )


def test_fresh_install_creates_latest(server, env):
    server.add_release("0.0.1")
    r = run(env)
    assert r.returncode == 0, r.stderr
    assert latest_version(env) == "0.0.1"
    exe = env["home"] / "bin" / "0.0.1" / BIN_NAME
    assert exe.is_file()
    assert exe.read_bytes() == b"binary 0.0.1"
    assert (env["home"] / "bin" / "0.0.1" / "catalog" / "pack.json").is_file()
    assert not (env["home"] / "bin" / "0.0.1.tmp").exists()
    if not IS_WINDOWS:
        assert os.access(str(exe), os.X_OK)
        assert (env["home"] / "bin" / "latest").is_symlink()


def test_second_run_is_noop_and_reports_newer(server, env):
    server.add_release("0.0.1")
    assert run(env).returncode == 0
    server.add_release("0.0.2")
    before = snapshot(env["home"])
    r = run(env)
    assert r.returncode == 0, r.stderr
    assert "tge 0.0.1 is installed" in r.stdout
    assert "newer: 0.0.2" in r.stdout
    assert snapshot(env["home"]) == before
    assert latest_version(env) == "0.0.1"


def test_check_changes_nothing(server, env):
    server.add_release("0.0.1")
    assert run(env).returncode == 0
    server.add_release("0.0.3")
    before = snapshot(env["home"])
    r = run(env, "--check")
    assert r.returncode == 0, r.stderr
    assert "0.0.1" in r.stdout
    assert "0.0.3" in r.stdout
    assert snapshot(env["home"]) == before


def test_update_repoints_latest(server, env):
    server.add_release("0.0.1")
    assert run(env).returncode == 0
    server.add_release("0.0.2")
    r = run(env, "--update")
    assert r.returncode == 0, r.stderr
    assert latest_version(env) == "0.0.2"
    assert (env["home"] / "bin" / "0.0.1" / BIN_NAME).is_file()
    again = run(env, "-u")
    assert again.returncode == 0
    assert "up to date" in again.stdout


def test_bad_checksum_aborts_and_keeps_latest(server, env):
    server.add_release("0.0.1")
    assert run(env).returncode == 0
    server.add_release("0.0.2", checksum="0" * 64)
    before = snapshot(env["home"])
    r = run(env, "--update")
    assert r.returncode == 1
    assert r.stderr.startswith("error:")
    assert latest_version(env) == "0.0.1"
    assert snapshot(env["home"]) == before


def test_prerelease_used_only_without_stable(server, env):
    pick = INSTALL.pick_newest
    rel = lambda tag, pre=False, draft=False: {  # noqa: E731
        "tag_name": tag,
        "prerelease": pre,
        "draft": draft,
    }
    only_pre = [rel("v0.2.0-rc.1", pre=True), rel("v0.1.0-beta.1", pre=True)]
    assert pick(only_pre)["tag_name"] == "v0.2.0-rc.1"
    mixed = only_pre + [rel("v0.1.0"), rel("v0.3.0", draft=True), rel("nightly")]
    assert pick(mixed)["tag_name"] == "v0.1.0"
    assert pick([rel("v0.3.0", draft=True), rel("junk")]) is None

    server.add_release("0.5.0", prerelease=True)
    r = run(env)
    assert r.returncode == 0, r.stderr
    assert latest_version(env) == "0.5.0"


def test_older_than_plugin_warns(server, env):
    server.add_release("0.0.1")
    assert run(env).returncode == 0
    (env["plugin"] / "VERSION").write_text("0.1.0\n")
    for args in ([], ["--check"]):
        r = run(env, *args)
        assert r.returncode == 0, r.stderr
        assert (
            "warning: installed tge 0.0.1 is older than this plugin (0.1.0); "
            "run install.py --update"
        ) in r.stdout


def test_archive_with_parent_path_is_rejected(server, env):
    top = "tge-v0.0.1-%s" % PLATFORM
    evil = build_archive(
        "0.0.1",
        {top + "/" + BIN_NAME: b"x", top + "/../../evil.txt": b"pwned"},
    )
    server.add_release("0.0.1", archive=evil)
    r = run(env)
    assert r.returncode == 1
    assert r.stderr.startswith("error:")
    assert not (env["home"] / "evil.txt").exists()
    assert not (env["home"] / "bin" / "0.0.1").exists()
    assert not (env["home"] / "bin" / "latest").exists()


@pytest.mark.skipif(IS_WINDOWS, reason="tar symlink member")
def test_archive_with_symlink_is_rejected(server, env):
    top = "tge-v0.0.1-%s" % PLATFORM
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        data = b"bin"
        info = tarfile.TarInfo(top + "/" + BIN_NAME)
        info.size = len(data)
        tf.addfile(info, io.BytesIO(data))
        link = tarfile.TarInfo(top + "/link")
        link.type = tarfile.SYMTYPE
        link.linkname = "/etc/passwd"
        tf.addfile(link)
    name = top + ".tar.gz"
    server.add_release("0.0.1", archive=(name, buf.getvalue()))
    r = run(env)
    assert r.returncode == 1
    assert r.stderr.startswith("error:")
    assert not (env["home"] / "bin" / "0.0.1").exists()
    assert not (env["home"] / "bin" / "0.0.1.tmp").exists()


@pytest.mark.skipif(IS_WINDOWS, reason="junction semantics differ")
def test_latest_as_real_directory_is_refused(server, env):
    server.add_release("0.0.1")
    real = env["home"] / "bin" / "latest"
    real.mkdir(parents=True)
    (real / "keep.txt").write_text("keep")
    r = run(env, "--update")
    assert r.returncode == 1
    assert r.stderr.startswith("error:")
    assert "latest" in r.stderr
    assert (real / "keep.txt").read_text() == "keep"
    assert not real.is_symlink()
    assert not (env["home"] / "bin" / "0.0.1").exists()


def test_python38_syntax():
    ast.parse(SCRIPT.read_text(encoding="utf-8"), feature_version=(3, 8))


TOP = "tge-v0.1.0-windows-x64"


@pytest.mark.parametrize(
    "name",
    [
        TOP + "/D:/evil.exe",
        TOP + "\\D:evil.exe",
        TOP + "/tge.exe:stream",
        TOP + "/.. /evil",
        TOP + "/evil.",
        TOP + "/a?b",
        "C:/x",
        "/abs/x",
        TOP + "/../x",
    ],
)
def test_split_member_rejects_unsafe_names(name):
    with pytest.raises(INSTALL.InstallError):
        INSTALL.split_member(name)


def test_plan_members_accepts_normal_layout():
    entries = [
        (TOP + "/", False, True, None),
        (TOP + "/tge.exe", True, False, "a"),
        (TOP + "/catalog/pack.json", True, False, "b"),
    ]
    plan = INSTALL.plan_members(entries, TOP)
    assert [p[0] for p in plan] == [["tge.exe"], ["catalog", "pack.json"]]


def make_zip(entries):
    """entries: list of (name, data, external_attr or None)."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        for name, data, attr in entries:
            if attr is None:
                zf.writestr(name, data)
            else:
                info = zipfile.ZipInfo(name)
                info.external_attr = attr
                zf.writestr(info, data)
    return buf.getvalue()


def test_read_zip_accepts_writestr_permission_only_mode():
    data = make_zip([(TOP + "/tge.exe", b"x", None), (TOP + "/catalog/", b"", None)])
    plan = INSTALL.read_zip(data, TOP)
    assert (["tge.exe"], False) in [(p[0], p[1]) for p in plan]


def test_read_zip_accepts_zero_mode_bits():
    data = make_zip([(TOP + "/tge.exe", b"x", 0)])
    assert INSTALL.read_zip(data, TOP)[0][0] == ["tge.exe"]


def test_read_zip_rejects_symlink_mode():
    link_attr = (0o120777) << 16
    data = make_zip([(TOP + "/tge.exe", b"x", None), (TOP + "/link", b"/etc/passwd", link_attr)])
    with pytest.raises(INSTALL.InstallError):
        INSTALL.read_zip(data, TOP)


@pytest.mark.skipif(sys.platform != "win32", reason="Windows directory links")
def test_windows_update_replaces_link(server, env):
    server.add_release("0.0.1")
    assert run(env).returncode == 0
    server.add_release("0.0.2")
    assert run(env, "--update").returncode == 0
    assert latest_version(env) == "0.0.2"
    assert (env["home"] / "bin" / "latest" / BIN_NAME).is_file()
