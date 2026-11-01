# /// script
# requires-python = ">=3.8"
# dependencies = []
# ///
"""Install and update the tge binary from GitHub Releases.

Binaries live in <home>/bin/<version>/ and <home>/bin/latest links to the
active one. Standard library only, Python 3.8+.
"""
import argparse
import hashlib
import io
import json
import os
import platform
import re
import shutil
import stat
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

DEFAULT_API = "https://api.github.com/repos/heuricore/tg-premium-emoji/releases"
HTTP_TIMEOUT = 60
UNSAFE_COMPONENT = re.compile(r'[:<>"|?*\x00-\x1f]')
SEMVER_TAG = re.compile(r"^v(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$")


class InstallError(Exception):
    pass


def platform_id():
    machine = platform.machine().lower()
    arch = {"x86_64": "x64", "amd64": "x64", "aarch64": "arm64", "arm64": "arm64"}.get(machine)
    if sys.platform.startswith("linux"):
        os_name, allowed = "linux", ("x64", "arm64")
    elif sys.platform == "darwin":
        os_name, allowed = "macos", ("x64", "arm64")
    elif sys.platform in ("win32", "cygwin"):
        os_name, allowed = "windows", ("x64",)
    else:
        os_name, allowed = None, ()
    if os_name is None or arch not in allowed:
        raise InstallError("unsupported platform: %s/%s" % (sys.platform, machine))
    return "%s-%s" % (os_name, arch)


def home_dir():
    custom = os.environ.get("TG_PREMIUM_EMOJI_HOME")
    return Path(custom) if custom else Path.home() / ".tg-premium-emoji"


def parse_version(text):
    """Parse 'MAJOR.MINOR.PATCH[-pre]' into a sortable key, or None."""
    match = SEMVER_TAG.match("v" + text)
    if not match:
        return None
    major, minor, patch, pre = match.groups()
    if pre is None:
        return (int(major), int(minor), int(patch), 1, ())
    ids = tuple((0, int(p), "") if p.isdigit() else (1, 0, p) for p in pre.split("."))
    return (int(major), int(minor), int(patch), 0, ids)


def release_version(release):
    """The version string of a release, or None for drafts and non-semver tags."""
    if release.get("draft"):
        return None
    tag = release.get("tag_name") or ""
    if not SEMVER_TAG.match(tag):
        return None
    return tag[1:]


def is_prerelease(release, version):
    return bool(release.get("prerelease")) or "-" in version


def http_get(url):
    request = urllib.request.Request(url, headers={"User-Agent": "tg-premium-emoji-install"})
    try:
        with urllib.request.urlopen(request, timeout=HTTP_TIMEOUT) as response:
            return response.read()
    except (OSError, ValueError) as exc:
        raise InstallError("cannot fetch %s: %s" % (url, exc))


def list_releases(api_url):
    try:
        data = json.loads(http_get(api_url).decode("utf-8"))
    except ValueError as exc:
        raise InstallError("invalid releases response from %s: %s" % (api_url, exc))
    if not isinstance(data, list):
        raise InstallError("invalid releases response from %s: expected a list" % api_url)
    return data


def candidates(releases):
    """Non-draft semver releases: stable ones, or pre-releases when no stable exists."""
    stable, pre = [], []
    for release in releases:
        version = release_version(release)
        if version is None:
            continue
        (pre if is_prerelease(release, version) else stable).append((parse_version(version), release))
    chosen = stable or pre
    return sorted(chosen, key=lambda item: item[0])


def pick_newest(releases):
    ranked = candidates(releases)
    return ranked[-1][1] if ranked else None


def find_asset(release, name):
    for asset in release.get("assets") or []:
        if asset.get("name") == name and asset.get("browser_download_url"):
            return asset["browser_download_url"]
    raise InstallError("release %s has no asset %s" % (release.get("tag_name"), name))


def split_member(name):
    """Split an archive member name into safe components, or raise."""
    if "\x00" in name:
        raise InstallError("unsafe archive member name: %r" % name)
    normalized = name.replace("\\", "/")
    if normalized.startswith("/") or re.match(r"^[A-Za-z]:", normalized):
        raise InstallError("unsafe archive member path: %s" % name)
    parts = [p for p in normalized.split("/") if p not in ("", ".")]
    if ".." in parts:
        raise InstallError("unsafe archive member path: %s" % name)
    for part in parts:
        if UNSAFE_COMPONENT.search(part) or part[-1] in ". ":
            raise InstallError("unsafe archive member path: %s" % name)
    return parts


def plan_members(entries, top):
    """Validate (name, is_file, is_dir, ref) entries; return [(parts, is_dir, ref)]."""
    plan = []
    for name, is_file, is_dir, ref in entries:
        parts = split_member(name)
        if not (is_file or is_dir):
            raise InstallError("archive member %s is not a regular file or directory" % name)
        if not parts:
            continue
        if parts[0] != top:
            raise InstallError("archive must contain only the top folder %s/" % top)
        if len(parts) == 1:
            if is_file:
                raise InstallError("archive top-level entry %s must be a folder" % top)
            continue
        plan.append((parts[1:], is_dir, ref))
    if not plan:
        raise InstallError("archive does not contain the folder %s/" % top)
    return plan


def read_tar(data, top):
    try:
        tf = tarfile.open(fileobj=io.BytesIO(data), mode="r:*")
    except tarfile.TarError as exc:
        raise InstallError("cannot read archive: %s" % exc)
    members = tf.getmembers()
    entries = [(m.name, m.isreg(), m.isdir(), m) for m in members]
    plan = plan_members(entries, top)
    return [(parts, is_dir, None if is_dir else tf.extractfile(m)) for parts, is_dir, m in plan]


def read_zip(data, top):
    try:
        zf = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile as exc:
        raise InstallError("cannot read archive: %s" % exc)
    entries = []
    for info in zf.infolist():
        kind = stat.S_IFMT(info.external_attr >> 16)
        is_dir = info.filename.endswith("/")
        if kind == 0:
            is_file = not is_dir
        else:
            is_file, is_dir = kind == stat.S_IFREG, kind == stat.S_IFDIR
        entries.append((info.filename, is_file, is_dir, info))
    plan = plan_members(entries, top)
    return [(parts, is_dir, None if is_dir else zf.open(info)) for parts, is_dir, info in plan]


def write_tree(plan, dest):
    dest.mkdir(parents=True)
    root = os.path.abspath(str(dest))
    for parts, is_dir, source in plan:
        target = dest.joinpath(*parts)
        if os.path.commonpath([root, os.path.abspath(str(target))]) != root:
            raise InstallError("archive member escapes the install directory: %s" % "/".join(parts))
        if is_dir:
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        with open(str(target), "wb") as out:
            shutil.copyfileobj(source, out)


def exe_name(plat):
    return "tge.exe" if plat.startswith("windows") else "tge"


def install(release, home):
    """Download, verify and unpack a release into bin/<version>; return that path."""
    version = release_version(release)
    if version is None:
        raise InstallError("release %s is not a usable version" % release.get("tag_name"))
    plat = platform_id()
    exe = exe_name(plat)
    bin_dir = Path(home) / "bin"
    final = bin_dir / version
    if (final / exe).is_file():
        return final

    top = "tge-v%s-%s" % (version, plat)
    archive_name = top + (".zip" if plat.startswith("windows") else ".tar.gz")
    data = http_get(find_asset(release, archive_name))
    checksum_text = http_get(find_asset(release, archive_name + ".sha256")).decode("utf-8", "replace")
    tokens = checksum_text.split()
    expected = tokens[0].lower() if tokens else ""
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        raise InstallError("checksum mismatch for %s (expected %s, got %s)" % (archive_name, expected, actual))

    reader = read_zip if archive_name.endswith(".zip") else read_tar
    plan = reader(data, top)
    if not any(parts == [exe] and not is_dir for parts, is_dir, _ in plan):
        raise InstallError("archive does not contain %s/%s" % (top, exe))

    tmp = bin_dir / (version + ".tmp")
    try:
        if tmp.exists():
            shutil.rmtree(str(tmp))
        write_tree(plan, tmp)
        if not plat.startswith("windows"):
            os.chmod(str(tmp / exe), 0o755)
        if final.exists():
            shutil.rmtree(str(final))
        os.rename(str(tmp), str(final))
    except OSError as exc:
        shutil.rmtree(str(tmp), ignore_errors=True)
        raise InstallError("cannot install into %s: %s" % (final, exc))
    except BaseException:
        shutil.rmtree(str(tmp), ignore_errors=True)
        raise
    return final


def is_link(path):
    """True for a symlink or a Windows junction (a reparse point)."""
    try:
        info = os.lstat(str(path))
    except OSError:
        return False
    if stat.S_ISLNK(info.st_mode):
        return True
    return bool(getattr(info, "st_file_attributes", 0) & 0x400)


def make_windows_link(target, link):
    """Create a directory link: a symlink if allowed, else a junction."""
    try:
        os.symlink(str(target), str(link), target_is_directory=True)
        return
    except (OSError, NotImplementedError):
        pass
    create_junction = None
    if os.name == "nt":
        import _winapi

        create_junction = getattr(_winapi, "CreateJunction", None)
    if create_junction is None:
        raise InstallError("cannot create a directory link %s: no symlink privilege and no junction support" % link)
    try:
        create_junction(os.path.abspath(str(target)), str(link))
    except OSError as exc:
        raise InstallError("cannot create junction %s: %s" % (link, exc))


def point_latest_windows(bin_dir, ver):
    latest = bin_dir / "latest"
    temp = bin_dir / ("latest.tmp-%d" % os.getpid())
    if os.path.lexists(str(temp)):
        os.rmdir(str(temp))
    make_windows_link(bin_dir / ver, temp)
    try:
        if os.path.lexists(str(latest)):
            os.rmdir(str(latest))
        os.rename(str(temp), str(latest))
    except OSError:
        os.rmdir(str(temp))
        raise


def check_latest_is_link(home):
    latest = Path(home) / "bin" / "latest"
    if latest.is_dir() and not is_link(latest):
        raise InstallError(
            "%s is a real directory, not a link; move or delete it and run install.py again "
            "(it must stay a link so 'tge doctor --prune' keeps the right version)" % latest
        )


def point_latest(home, ver):
    bin_dir = Path(home) / "bin"
    latest = bin_dir / "latest"
    target = bin_dir / ver
    check_latest_is_link(home)
    try:
        if os.name == "nt":
            point_latest_windows(bin_dir, ver)
        else:
            temp = bin_dir / ".latest.tmp"
            if os.path.lexists(str(temp)):
                os.unlink(str(temp))
            os.symlink(ver, str(temp))
            os.replace(str(temp), str(latest))
    except OSError as exc:
        raise InstallError("cannot link %s -> %s: %s" % (latest, target, exc))


def installed_version(home):
    latest = Path(home) / "bin" / "latest"
    if not latest.is_dir():
        return None
    return os.path.basename(os.path.realpath(str(latest)))


def plugin_version():
    try:
        return (Path(__file__).resolve().parent.parent / "VERSION").read_text(encoding="utf-8").strip()
    except OSError:
        return None


def warn_if_older(installed):
    plugin = plugin_version()
    if not plugin or not installed:
        return
    have, want = parse_version(installed), parse_version(plugin)
    if have is not None and want is not None and have < want:
        print(
            "warning: installed tge %s is older than this plugin (%s); run install.py --update"
            % (installed, plugin)
        )


def newer_than(releases, installed):
    key = parse_version(installed) if installed else None
    found = [(k, r) for k, r in candidates(releases) if key is None or k > key]
    return [release_version(r) for _, r in reversed(found)]


def do_install_newest(releases, home):
    release = pick_newest(releases)
    if release is None:
        raise InstallError("no usable releases found")
    path = install(release, home)
    point_latest(home, path.name)
    print("tge %s installed at %s" % (path.name, path))
    return path.name


def run(args):
    home = home_dir()
    api_url = os.environ.get("TGE_RELEASES_API") or DEFAULT_API
    check_latest_is_link(home)
    installed = installed_version(home)

    if args.update:
        releases = list_releases(api_url)
        newest = pick_newest(releases)
        if newest is None:
            raise InstallError("no usable releases found")
        newest_ver = release_version(newest)
        have, want = (parse_version(installed) if installed else None), parse_version(newest_ver)
        if have is not None and have >= want:
            print("tge %s is up to date" % installed)
        else:
            installed = do_install_newest(releases, home)
    elif args.check:
        releases = list_releases(api_url)
        if installed:
            print("tge %s is installed" % installed)
        else:
            print("tge is not installed")
        newer = newer_than(releases, installed)
        if newer:
            print("newer: %s" % ", ".join(newer))
    elif installed is None:
        installed = do_install_newest(list_releases(api_url), home)
    else:
        print("tge %s is installed" % installed)
        try:
            newer = newer_than(list_releases(api_url), installed)
        except InstallError as exc:
            print("warning: cannot check for newer releases: %s" % exc, file=sys.stderr)
            newer = []
        if newer:
            print("newer: %s" % ", ".join(newer))

    warn_if_older(installed)


def main(argv=None):
    parser = argparse.ArgumentParser(description="Install or update the tge binary.")
    parser.add_argument("--check", action="store_true", help="show installed and newer versions, change nothing")
    parser.add_argument("-u", "--update", action="store_true", help="install the newest release and repoint latest")
    args = parser.parse_args(argv)
    try:
        run(args)
    except InstallError as exc:
        print("error: %s" % exc, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
