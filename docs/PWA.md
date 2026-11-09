# Browser workbench

The PWA runs the existing Rust planner, validation, stock simulation, model parsers,
and postprocessors in WebAssembly inside a worker. Files are not uploaded to a
server. A responsive HTML interface supports phone, tablet and desktop layouts.
This is an initial browser workbench, not full parity with the native UI.

## Build and run

Prerequisites: stable Rust, Python 3, and Node/npm for browser tests.

```sh
bash scripts/build-web.sh
python3 -m http.server 8080 --directory web
```

Open the local server on the development computer. For tests, in another terminal:

```sh
cd web
npm ci
npx playwright install chromium
CHROMIUM_PATH="$(node --input-type=module -e "import {chromium} from 'playwright'; console.log(chromium.executablePath())")" npm test
```

The build script installs the matching pinned wasm-bindgen CLI and target.
Generated `web/pkg` files are ignored and must be included in deployments.

## Try it on a phone or tablet

Host the contents of `web` (excluding node_modules and test files, including pkg)
on any static HTTPS host. Relative paths support a repository subdirectory.
No backend or credentials are needed. Plain HTTP on a LAN address will not enable
service workers or PWA installation; use HTTPS for real-device testing.

The **PWA build and optional deployment** GitHub workflow builds, tests and uploads
a site artifact. To deploy it, enable repository Settings → Pages → Deploy from a branch → gh-pages → / (root),
then run the workflow with **deploy** checked. Open the URL reported by that
workflow on the phone. Pushes to main deploy after the build and tests pass; manual deployment is also available.
On Android use the browser install option; on iOS use Share → Add to Home Screen.
Wait for “Offline cache installed” before going offline. Browser installation and
storage retention depend on browser support and available device storage.

## Available workflow

- Start a blank milling project or tray demo; open native `.cam.json` projects.
- Edit stock, legacy tool settings, postprocessor and operations.
- Save/download projects; the last changed workspace is restored from local storage.
  Download backups: browser storage can be cleared or evicted.
- Import STEP/IGES (file units converted to mm), or STL/OBJ with explicit source units. Each creates a fitted new project.
- Generate checked paths, inspect top/isometric views and scrub path playback.
- Plan from stock / 3D surfaces using the existing supported geometry restrictions.
- Run sampled stock-removal simulation and inspect the numeric report.
- Download G-code generated from current data; invalid paths block export.
- Edit other shared settings through the advanced JSON editor. Library-backed
  projects keep their tool assignments; the basic tool fields edit the legacy
  tool only. Edit the library through JSON or the native app.

## Current limits

STEP/IGES import runs locally through bundled OpenCascade WASM. It retains source
text and flattens assemblies to a tessellated mesh (0.1 mm linear / 0.3 radian
angular deflection). The kernel adds about 7.5 MiB to the offline download.
Inputs are limited to 32 MiB, output to 100,000 triangles / 300,000 vertices; an
import is stopped after 120 seconds. Very large CAD can still exceed mobile memory. CAD source plus mesh can exceed
local-storage quota: download the project if the app reports that autosave is unavailable.
A failed import leaves the current project intact. CAD import does not imply all
shapes are supported by automatic machining strategies. The browser
viewer currently shows model wireframe and toolpaths, rather than the native
GPU solid renderer or rendered cut-stock simulation. Detailed machine/library
editors, gesture navigation, native undo/redo, and full native UI parity remain
future work. Large models/plans can consume substantial mobile memory; work runs
in a worker so the page remains responsive. Direct controller communication is
out of scope.

The service worker caches one release atomically. Packaging derives the cache version from the shipped assets. An update waits until existing tabs close; reopen the
app to activate it. Never cache user projects or G-code as HTTP assets.

## Self-hosting

Build with `bash scripts/build-web.sh`, then copy these files to your server's
static document root (for example `/srv/opensourcecam`):

```
index.html style.css app.js worker.js sw.js manifest.webmanifest
icon.svg icon-192.png icon-512.png build.js cad-worker.js pkg/ cad/
```

Do not serve the source checkout or `node_modules`. No Rust, Python, Node or CAD
runtime is needed on the hosting server. Ensure `.wasm` is served as
`application/wasm` (modern Caddy/Nginx/Apache support this).

`web/Caddyfile` provides a minimal Caddy configuration. Replace `cam.example.com`
with your domain, point its DNS at the server, and allow ports 80/443. Caddy can
then manage HTTPS automatically. Existing HTTPS static hosts work equally well.
For private LAN hosting, use a certificate trusted by the phone/tablet; a
self-signed certificate warning is not sufficient for reliable PWA support.

Deploy all assets together. Avoid long-lived immutable cache headers on unversioned
files. For upgrades, change the service worker cache version, deploy the rebuilt
assets, and close/reopen existing app tabs. Keep project downloads as backups;
changing the website origin does not migrate the browser's saved workspace.

## Shop development launcher

Download/extract `opensourcecam-shop.zip`. Install Python 3.10+ once, then
Windows users can double-click `Start-Shop.bat`; Linux/macOS users run
`sh Start-Shop.sh`. The browser opens automatically. Keep the host window open.
Phones/tablets on the same Wi-Fi use the LAN address printed in that window.
The host serves only the packaged site, not the repository or user projects.
No Rust/npm setup is needed for users of the package.

This development package still requires Python; a standalone installer remains
future work. LAN HTTP enables online testing, but PWA installation and offline
support on mobile require trusted HTTPS. Use GitHub Pages for immediate HTTPS
remote tests, or configure trusted shop HTTPS as described above. The launcher
also accepts `--cert` and `--key` for an existing trusted certificate pair.
No login is provided; use a trusted shop LAN and do not forward the port publicly.

Developers regenerate both ZIPs with `python3 scripts/package-web.py` after the
WebAssembly build. The generated `dist/shop` folder can be tested with
`python3 dist/shop/serve.py --bind 127.0.0.1 --port 8081 --no-browser`.

## Updates and project recovery

The footer shows the content-based build version. New releases download in the
background and offer **Update available · reload**. Applying an update saves the
current workspace first; if saving fails, download a draft backup before updating.
The first upgrade from the original prototype may require closing all app tabs
and reopening, because that version had no update prompt.

Committed field changes are autosaved locally, with one previous snapshot.
**Previous autosave** restores that snapshot, subject to project validation.
**Undo/Redo** retain up to 30 recent changes within an 8 MiB history budget during
the session. History does not survive reloads. **Download draft backup** also
works for incomplete/invalid projects. Unreadable stored drafts are preserved and
can be downloaded using **Recover draft**. Browser storage is not a permanent
backup: keep downloaded copies and avoid editing the same project in multiple tabs.

The build/test workflow publishes to the existing gh-pages branch and explicitly
requests a Pages build. Run `python3 scripts/package-web.py` then
`node web/reliability.mjs` to exercise a real service-worker upgrade, offline CAM,
undo/redo, reload recovery, corrupt drafts and storage-quota failure handling.

Operation editing and Undo/Redo now run in the Rust session inside the worker.
The operation inspector also supports duplication and moving operations up/down.
Incomplete typed autosave drafts can be restored without bypassing machining checks;
Generate and Download G-code still reject invalid setups. Native desktop edit/history
migration to this session is not complete.
