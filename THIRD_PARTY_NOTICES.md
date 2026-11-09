# Bundled CAD importer

The Rust workbench is MIT licensed. The complete desktop package also ships a
private CPython runtime and dynamically loaded OpenCascade bindings. These are
separate components and keep their upstream licenses. No FreeCAD installation,
system Python, network access or separate CAD application is required by users.

- **Open CASCADE Technology 7.9.3**: LGPL-2.1 with the OCCT additional exception.
  [Source, license and exception](https://github.com/Open-Cascade-SAS/OCCT/tree/V7_9_3).
- **cadquery-ocp-novtk / OCP 7.9.3.1.1**: generated bindings distributed under
  Apache-2.0; bundled OCCT libraries retain OCCT's license.
  [Source and build instructions](https://github.com/CadQuery/OCP),
  [exact binary distribution](https://pypi.org/project/cadquery-ocp-novtk/7.9.3.1.1/).
- **cadquery-ocp-proxy 7.9.3.1.1**:
  [source and distribution](https://pypi.org/project/cadquery-ocp-proxy/7.9.3.1.1/).
- **CPython 3.12.10**: Python Software Foundation license and bundled component
  licenses. [Source](https://www.python.org/downloads/release/python-31210/).
  Portable builds come from [python-build-standalone](https://github.com/astral-sh/python-build-standalone).

The CAD kernel's supporting libraries also include FreeImage, FreeType, libdeflate,
OpenEXR/Imath, libjpeg-turbo, LittleCMS, Lerc, liblzma, libpng, libwebp, OpenJPEG,
OpenJPH, LibRaw, libtiff, zlib and Zstandard. Python also includes OpenSSL, libffi,
SQLite and its standard bundled components. Upstream license snapshots and source
links are included in `cad-runtime/licenses` and `sources.json`; CPython's own
license is retained in `cad-runtime/python/LICENSE.txt`. Windows distributions
include the upstream wheels' Microsoft compiler runtime support DLLs; those keep
their Microsoft redistribution terms and are platform support, not CAD libraries.
The compiled Rust dependencies' notices and source inventory are in `licenses/rust`.

The CAD kernel stays dynamically loaded from
`cad-runtime/packages`; compatible rebuilt versions can replace those libraries.
Exact wheel hashes are recorded in `cad-runtime/cad-requirements.txt`. The
runtime's versions and platform are recorded in `cad-runtime/manifest.json`.
The app source, bridge, tests, and reproducible packaging script also ship in `source/`.

## Browser CAD kernel

The PWA separately distributes **occt-import-js 0.0.23** (LGPL-2.1) with its
OpenCascade WebAssembly kernel (LGPL-2.1 with OCCT exception). Full notices,
corresponding source revisions and replacement/build information are included in
`cad/NOTICES.txt`, `cad/license.occt-import-js.txt`, `cad/license.occt.txt`
and `cad/license.occt-exception.txt`.
The unmodified npm distribution is pinned with its integrity hash in
`web/package-lock.json`. See [browser kernel notices](web/CAD-NOTICES.txt).
