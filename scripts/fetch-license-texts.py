"""Maintainer utility: refresh redistributable license texts from upstream sources.

Builds copy the checked-in license snapshots; this utility is never run by users.
"""
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
SOURCES = {
    "OCCT-LGPL-2.1.txt": "https://raw.githubusercontent.com/Open-Cascade-SAS/OCCT/V7_9_3/LICENSE_LGPL_21.txt",
    "OCCT-exception.txt": "https://raw.githubusercontent.com/Open-Cascade-SAS/OCCT/V7_9_3/OCCT_LGPL_EXCEPTION.txt",
    "OCP-Apache-2.0.txt": "https://raw.githubusercontent.com/CadQuery/OCP/master/LICENSE",
    "libdeflate.txt": "https://raw.githubusercontent.com/ebiggers/libdeflate/master/COPYING",
    "FreeImage.txt": "https://raw.githubusercontent.com/WinMerge/freeimage/master/license-fi.txt",
    "FreeType.txt": "https://raw.githubusercontent.com/freetype/freetype/VER-2-12-1/docs/FTL.TXT",
    "OpenEXR.txt": "https://raw.githubusercontent.com/AcademySoftwareFoundation/openexr/main/LICENSE.md",
    "Imath.txt": "https://raw.githubusercontent.com/AcademySoftwareFoundation/Imath/main/LICENSE.md",
    "libjpeg-turbo.txt": "https://raw.githubusercontent.com/libjpeg-turbo/libjpeg-turbo/main/LICENSE.md",
    "libjpeg-IJG.txt": "https://raw.githubusercontent.com/libjpeg-turbo/libjpeg-turbo/main/README.ijg",
    "LittleCMS.txt": "https://raw.githubusercontent.com/mm2/Little-CMS/master/LICENSE",
    "Lerc.txt": "https://raw.githubusercontent.com/Esri/lerc/master/LICENSE",
    "liblzma.txt": "https://raw.githubusercontent.com/tukaani-project/xz/master/COPYING.0BSD",
    "libpng.txt": "https://raw.githubusercontent.com/pnggroup/libpng/libpng16/LICENSE",
    "libwebp.txt": "https://raw.githubusercontent.com/webmproject/libwebp/main/COPYING",
    "libwebp-PATENTS.txt": "https://raw.githubusercontent.com/webmproject/libwebp/main/PATENTS",
    "OpenJPEG.txt": "https://raw.githubusercontent.com/uclouvain/openjpeg/master/LICENSE",
    "OpenJPH.txt": "https://raw.githubusercontent.com/aous72/OpenJPH/master/LICENSE",
    "LibRaw.txt": "https://raw.githubusercontent.com/LibRaw/LibRaw/master/LICENSE.LGPL",
    "LibRaw-COPYRIGHT.txt": "https://raw.githubusercontent.com/LibRaw/LibRaw/master/COPYRIGHT",
    "libtiff.txt": "https://gitlab.com/libtiff/libtiff/-/raw/master/LICENSE.md",
    "zlib.txt": "https://raw.githubusercontent.com/madler/zlib/develop/LICENSE",
    "zstd.txt": "https://raw.githubusercontent.com/facebook/zstd/v1.5.7/LICENSE",
    "OpenSSL.txt": "https://raw.githubusercontent.com/openssl/openssl/openssl-3.0/LICENSE.txt",
    "libffi.txt": "https://raw.githubusercontent.com/libffi/libffi/master/LICENSE",
    "SQLite.txt": "https://www.sqlite.org/copyright.html",
    "Python-standalone.txt": "https://raw.githubusercontent.com/astral-sh/python-build-standalone/main/LICENSE",
}


def main():
    output = ROOT / "licenses/third-party"
    output.mkdir(parents=True, exist_ok=True)
    manifest = {}
    failures = []
    for name, url in SOURCES.items():
        try:
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read(512 * 1024)
            if len(data) < 50:
                raise ValueError("Empty license response")
            (output / name).write_bytes(data)
            manifest[name] = {"source": url, "sha256": hashlib.sha256(data).hexdigest()}
        except Exception as error:
            failures.append(f"{name}: {error}")
    (output / "sources.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"Saved {len(manifest)} upstream license snapshots")


if __name__ == "__main__":
    main()
