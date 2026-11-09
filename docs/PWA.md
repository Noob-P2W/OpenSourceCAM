# Browser workbench

The PWA runs the existing Rust planner, validation, stock simulation, model parsers,
and postprocessors in WebAssembly inside a worker. Files are not uploaded to a
server. A responsive HTML interface supports phone, tablet and desktop layouts.
This is an initial browser workbench, not full parity with the native UI.

## Build and run

Prerequisites: stable Rust, Python 3, and Node/npm for browser tests.

```sh
npm ci --prefix web
bash scripts/build-web.sh
python3 scripts/package-web.py
python3 -m http.server 8080 --directory dist/pwa
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

Host the generated contents of `dist/pwa`
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

- Choose among the nine desktop processes on the opening page; Milling has the
  implemented motion engine. Other processes support reference models and saving.
- Follow Import → Setup → Operations on phone, tablet or desktop. File stays at
  the upper left; Processes returns to the opening page without replacing a job.
- Import STEP/IGES, STL or OBJ. A part creates a fitted project in the chosen
  process. Vise, fixture and CNC reference models join its workspace.
- Inspect a full-resolution shaded workpiece and stock box, orbit, pinch/scroll
  zoom, pan, choose top/front/isometric, or Fit view. Software fallback is sampled.
- Fit stock in Setup, inspect XYZ dimensions and placement, configure the CNC and
  workholding, select tools/presets and edit detailed cutting/machine settings.
- Manage all 37 tool definition kinds locally: search, create, edit dimensions,
  shaft/holder, cutting presets and post settings; duplicate or remove unassigned
  tools; save tool libraries and CNC profiles. Assembly previews use the native
  illustrative geometry helper.
- In Operations use **Plan 3D surface** for accessible upper surfaces, **Plan 2.5D
  from stock** for the supported complete-part shapes, or **Recognize features**
  for supported holes/pockets. Planning options include XY/top/backing allowances,
  explicit operation replacement and the desktop 2.5D choices. Notes remain visible.
- Add any implemented manual strategy; edit all operation settings, review
  geometry links, assign tools/presets, reorder, duplicate, enable/disable and undo.
- Generate checked paths, play/scrub the sequence and optionally filter its display
  to the selected operation. Render sampled stock removal at the playback progress
  and inspect the numeric report. Preview checked NC text and download G-code.
- Download/reopen native project JSON, restore local drafts and use offline updates.
  Browser storage can be evicted: keep downloaded backups of important work.

See FEATURE_PARITY.md for remaining native UI differences. None of these controls
adds machining for interior/undercut surfaces or enables planned process engines.

## Current limits

STEP/IGES import runs locally through bundled OpenCascade WASM. It retains source
text and flattens assemblies to a tessellated mesh (0.1 mm linear / 0.3 radian
angular deflection). The kernel adds about 7.5 MiB to the offline download.
Inputs are limited to 32 MiB, output to 100,000 triangles / 300,000 vertices; an
import is stopped after 120 seconds. Very large CAD can still exceed mobile memory. CAD source plus mesh can exceed
local-storage quota: download the project if the app reports that autosave is unavailable.
A failed import leaves the current project intact. CAD import does not imply all
shapes are supported by automatic machining strategies. The browser now has depth-tested solids, camera gestures, rendered sampled
stock and structured machine/library editors. Full native UI parity is still
tracked in FEATURE_PARITY.md; synchronous WASM jobs currently run to completion. Large models/plans can consume substantial mobile memory; work runs
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

## Import → Setup → Operations

The upper-left **File** menu holds New, Open, Save, the demo, backups, recovery,
and the advanced JSON editor. It closes on Escape or an outside click.

All screen sizes use the same three workflow steps:

1. **Import:** load a part, CNC reference mesh, vise or fixture model. Part import
   starts a fitted project; reference meshes are added without replacing the part.
   Import `.cam-machine.json` CNC profiles and `.cam-tools.json` tool libraries here.
   Use Continue to Setup once the required files are in.
2. **Setup:** fit stock to the part with an XY margin, edit stock dimensions and
   part offsets, select an imported CNC profile, and review travel limits, work
   origin, feed/spindle limits, reach checks, postprocessor, coolant and work offset.
   Position vise/fixture/reference meshes using translation, rotation and coordinate
   frame. Reference geometry is previewed using the shared Rust transforms.
   Select a library tool and cutting preset, or edit the legacy tool and convert
   it to a library tool. Imported profile choices last this browser session;
   the applied CNC settings are saved in the project. Tool imports merge copies
   using the desktop rules; explicit replacement is allowed only without existing
   operations. Machine configuration and changer settings have structured editors.
3. **Operations:** recognize supported features, review candidate holes/pockets,
   add disabled feature operations, or use stock/surface planning. New manual
   operations retain the Setup tool/preset, and each operation exposes its own
   tool/preset assignment. Enable reviewed
   operations, generate paths, simulate stock, inspect playback and download G-code.
   Recognition uses the existing conservative Rust implementation; it does not
   recognize arbitrary manufacturing features or guarantee machinability.

Fit stock aligns the part top to Z=0, applies the XY margin and sets thickness to
part height (0.001 mm minimum). Existing operations remain; review them and fixture
positions after refitting. Add backing/blank allowances manually where required.
CNC profiles are definitions, not controller connections. Verify actual machine
specifications. Vise/fixture meshes do not implement articulated machine simulation.

On phones (up to 640 px), the three steps use bottom navigation. The preview is
shown in every step and can be hidden explicitly on small screens. On larger
screens, the preview stays beside the selected step. Rotation retains edits.
Tests cover stock fitting in tablet Setup, CNC profile application, vise placement,
tool/preset selection, feature operations, file-menu placement, and screen overflow.

## Large STL handling

Identical STL vertices are shared exactly, reducing memory without changing faces.
Full-resolution workpiece solids render in WebGL. Wireframe/software fallback
previews sample large meshes; full mesh data is kept for CAM and saved projects.
GPU scene buffers are replaced after edits, and backing buffers remain bounded.
Large project JSON is loaded into the advanced editor only when explicitly opened.
Autosave uses compact JSON, and failure to refresh the previous backup no longer
prevents saving the current project. Keep downloaded backups, especially if the
storage warning says that the previous recovery copy could not be refreshed.
The 100,000-triangle import regression exercises preview, editing, undo and a
full-resolution save. It is a synthetic browser test, not verification on every
Brave/Android device; model topology and device memory still affect workload.
