r"""
Fetch libmpv-2.dll (and the mpv headers, for reference) into vendor\libmpv.

The player loads libmpv-2.dll from its own folder at run time (src/mpv/sys.rs),
so the build itself needs nothing; build.py, tools/make_release.py and the
release workflow call this script and copy the DLL next to the exe. The
package is a pinned build of shinchiro/mpv-winbuild-cmake, checked against
its SHA-256. shinchiro keeps only the last 30 builds (about two months), so
the archive is mirrored unchanged in a pre-release of this repository
(tag libmpv-<date>), tried first; the original is the fallback. It is packed with the BCJ2 filter, which neither py7zr nor the
tar of Windows can unpack, so the standalone 7zr.exe of 7-Zip (pinned and
checked as well) is fetched next to it. A second run does nothing when the
DLL of the pinned build is already there.

No external dependencies. Run from any folder: python tools/get_libmpv.py [--force]
"""

import argparse
import hashlib
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

# --- The pinned builds --------------------------------------------------------------
LIBMPV_ASSET = "mpv-dev-x86_64-20261012-git-88eb9f2e73.7z"
# The mirror first (a pre-release of this repository, made when the build is
# pinned: gh release create libmpv-<date> --prerelease ... <archive>), then
# the original, which goes away after about two months.
LIBMPV_URLS = [
    f"https://github.com/Fan4Metal/metalmpv/releases/download/libmpv-20261012/{LIBMPV_ASSET}",
    f"https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261012/{LIBMPV_ASSET}",
]
LIBMPV_SHA256 = "a739eef92a907dfb8ea997564a3fa565b2a4f376f20a25755e7645d0c70fe5e6"
# For THIRD_PARTY_LICENSES.md (the GPL's corresponding source): the commit of
# mpv-winbuild-cmake the release was built from (the tag's), and the FFmpeg
# commit the DLL reports (av_version_info, "N-127375-gf4012774b").
LIBMPV_BUILD_COMMIT = "1f8d2e76255abc48f0456d9be99b4cab9c6566e6"
LIBMPV_FFMPEG_COMMIT = "f4012774b"
SEVENZIP_URL = "https://github.com/ip7z/7zip/releases/download/26.03/7zr.exe"
SEVENZIP_SHA256 = "ad4c82fadcbdf93c03b4fc440f300509c7d60c5c2f4d183e35d9d70d6957037d"
# -------------------------------------------------------------------------------------

ROOT = Path(__file__).resolve().parent.parent
VENDOR = ROOT / "vendor"
LIBMPV = VENDOR / "libmpv"
DLL = LIBMPV / "libmpv-2.dll"
# Written last: the folder is complete only when it names the pinned build.
STAMP = LIBMPV / "VERSION"
SEVENZIP = VENDOR / "7zr.exe"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(1 << 20):
            digest.update(chunk)
    return digest.hexdigest()


class DownloadError(Exception):
    pass


def download(url: str, to: Path, expected: str) -> None:
    """Download `url` to `to` and check its SHA-256; a failure leaves nothing behind."""
    print(f"Downloading {url}", flush=True)
    request = urllib.request.Request(url, headers={"User-Agent": "metalmpv-get_libmpv"})
    partial = to.with_suffix(to.suffix + ".part")
    try:
        with urllib.request.urlopen(request, timeout=60) as answer, partial.open("wb") as out:
            shutil.copyfileobj(answer, out, 1 << 20)
    except (urllib.error.URLError, OSError) as e:
        partial.unlink(missing_ok=True)
        raise DownloadError(f"cannot download {url}: {e}") from e
    actual = sha256(partial)
    if actual != expected:
        partial.unlink()
        raise DownloadError(f"SHA-256 mismatch for {url}\n  expected {expected}\n  got      {actual}")
    partial.replace(to)


def download_any(urls: list[str], to: Path, expected: str) -> None:
    """Download from the first of `urls` that gives the file with the expected SHA-256."""
    errors = []
    for url in urls:
        try:
            download(url, to, expected)
            return
        except DownloadError as e:
            print(f"  {e}", flush=True)
            errors.append(str(e))
    raise SystemExit("ERROR: no source gave the file:\n  " + "\n  ".join(errors))


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch the pinned libmpv-2.dll into vendor\\libmpv")
    parser.add_argument("--force", action="store_true", help="download again even if it is there")
    args = parser.parse_args()

    if not args.force and DLL.is_file() and STAMP.is_file() and STAMP.read_text().strip() == LIBMPV_ASSET:
        print(f"libmpv is up to date: {DLL}")
        return 0

    VENDOR.mkdir(exist_ok=True)
    if not SEVENZIP.is_file() or sha256(SEVENZIP) != SEVENZIP_SHA256:
        download_any([SEVENZIP_URL], SEVENZIP, SEVENZIP_SHA256)

    with tempfile.TemporaryDirectory(dir=VENDOR) as tmp:
        archive = Path(tmp) / LIBMPV_ASSET
        download_any(LIBMPV_URLS, archive, LIBMPV_SHA256)
        extracted = Path(tmp) / "x"
        result = subprocess.run(
            [str(SEVENZIP), "x", "-y", f"-o{extracted}", str(archive), "libmpv-2.dll", "include"],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(result.stdout + result.stderr, file=sys.stderr)
            raise SystemExit("ERROR: 7zr could not unpack the archive")
        if LIBMPV.exists():
            shutil.rmtree(LIBMPV)
        LIBMPV.mkdir()
        shutil.move(extracted / "libmpv-2.dll", DLL)
        if (extracted / "include").is_dir():
            shutil.move(extracted / "include", LIBMPV / "include")
        STAMP.write_text(LIBMPV_ASSET + "\n")
    print(f"libmpv: {DLL} ({DLL.stat().st_size >> 20} MB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
