# Browser workbench

The PWA runs the existing Rust planner, validation, stock simulation, model parsers,
and postprocessors in WebAssembly inside a worker. Files are not uploaded to a
server. A responsive HTML interface supports phone, tablet and desktop layouts.
The browser is the primary development target. Native desktop development and
automatic native packaging are paused; existing native source remains available.
The shop-hosting ZIP and static-site package continue to be built and tested.

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
text and tessellates assemblies while retaining named component boundaries
(0.1 mm linear / 0.3 radian
angular deflection). The kernel adds about 7.5 MiB to the offline download.
Inputs are limited to 32 MiB, output to 250,000 triangles / 750,000 vertices; an
import is stopped after 120 seconds. Very large CAD can still exceed mobile memory. CAD source plus mesh can exceed
local-storage quota: download the project if the app reports that autosave is unavailable.
A failed import leaves the current project intact. CAD import does not imply all
shapes are supported by automatic machining strategies. The browser now has depth-tested solids, camera gestures, rendered sampled
stock and structured machine/library editors. Full native UI parity is still
tracked in FEATURE_PARITY.md. Cancel calculation restarts a worker and restores
the current workspace and its undo/redo history. Large models/plans can consume substantial mobile memory; work runs
in a worker so the page remains responsive. Direct controller communication is
out of scope.

The service worker caches one release atomically. Packaging derives the cache version from the shipped assets. An update waits until existing tabs close; reopen the
app to activate it. Never cache user projects or G-code as HTTP assets.

## Self-hosting

Build with `bash scripts/build-web.sh`, then copy these files to your server's
static document root (for example `/srv/opensourcecam`):

```
index.html style.css app.js shell.js viewport.js worker.js sw.js manifest.webmanifest
icon.svg icon-192.png icon-512.png build.js cad-worker.js assets/ pkg/ cad/
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

The desktop and tablet workbench uses a command ribbon above a large viewport,
with a project tree showing Setup 1 and its numbered operation steps. The optional
selection panel appears on wide screens. On phones, the project tree opens as a
drawer and the ribbon scrolls horizontally. Settings windows resize to the screen;
rotating the device retains the project and local operation draft.

1. **Import:** load a part, CNC reference mesh, vise or fixture. Importing a part
   starts a fitted project. Reference models are added to the current project.
2. **Setup:** choose a rectangular or round blank, edit physical dimensions and XYZ
   stock/part orientation, fit or position the part, and set clearance. The viewer
   starts in an axonometric projection; camera rotation never changes machining
   orientation. Choose a machine and default tool from the Libraries windows.
   Review machine travel, work origin, feed/spindle limits, tool reach, configuration,
   changer, postprocessor, work offset and coolant in its editors.
3. **Operations:** the 2.5D, 3D and Drilling ribbon tabs expose individual illustrated
   commands. Each operation opens a local draft with Geometry, Tool, Passes, Feeds
   and Heights tabs. Save validates and commits the draft as one undo step; Cancel
   discards it. Feeds and cutting overrides belong to that operation. Right-click
   an operation for editing, duplication, ordering or removal. Generate paths,
   inspect playback, simulate material removal, preview NC and export from Actions.

Tool and machine libraries use searchable tables. Tool definitions and presets
are stored in the project, with explicit device-library save/load and portable
JSON import/export. Custom machine records persist on this browser and include
both machine configuration and the selected postprocessor. The four generic
milling templates target Generic, GRBL, Mach3 and LinuxCNC output; they require
measured machine settings. Legacy `.cam-machine.json` profiles can be imported.
Libraries do not connect to a controller.

Workholding includes a configurable example vise with a base and two jaws. Set jaw
width/depth, grip and part clearance, and reserve backing below the finished part.
Reseating replaces that example's geometry. Imported vise/fixture meshes use editable
translations, rotations and work/machine coordinate frames. Recheck workholding after
stock changes. This remains static reference geometry with conservative fixture
checks, rather than articulated machine simulation.

Stock fitting preserves the chosen blank shape and orientation. Model-based
planning uses the stock and placement from Setup by default. Disable that option
only when fitting a new blank with XY/top/backing allowances. Existing operations
remain until explicit replacement; geometry links become stale after part placement
or orientation changes, and generation/export require rebuilding them. Round/tilted
stock removal uses sampled physical columns, with the existing preview resource limit.

Project naming is in Project properties under File or the header title. Processes
is at the upper left. Software Settings controls theme, density, default projection,
import units, grid, axes, rapid visibility, framing and the wide-screen selection panel.
Recognition retains its conservative supported-feature limits; other process workspaces
remain reference-only until their motion engines are implemented.

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

## Playback, dialogs and tool dimensions

The **Operations** button opens **Project & operations** on phones. It shows Setup
and the operation list; desktop/tablet keep the list visible. Application decisions
such as replacing a workspace use themed pop-up dialogs instead of native browser
confirmations. Dark mode resolves foreground and button colors in the selected theme.

With **Simulated stock** enabled, Play starts with uncut stock and updates removal
at the current playback time. Rewind/scrub discard later stock snapshots. Playback
defaults to a bounded 32-cell preview, with higher playback Detail settings available; choose **Actions → Stock simulation** for the detailed
inspection at the current slider position. The stock label shows the actual displayed
percentage while a worker update is pending. Toolpath motion and removal are sampled
against the same Rust-generated timeline.

**Show tool** is enabled by default in Toolpath playback. It draws the operation's
library cutter, shaft and configured holder at actual scale, with the tip following
the path. The model switches at tool changes; the viewport label identifies the
current tool. It works with simulated stock and in software graphics fallback.
Disable Show tool to use the smaller position marker. These are the same illustrative
assembly surfaces shown by the library; they do not replace the shared cutting and
fixture checks. Older projects without a library use their legacy setup dimensions.

**Drilling → Drill hole** creates a single-feed cycle; **Peck drilling** creates a
full-retract chip-clearing cycle. Select a measured drill from the library and review
its feed, tip depth, R plane and peck settings. See [drilling and cutter offsets](DRILLING_AND_COMPENSATION.md).

In the tool definition window, focus a diameter or length field to show a blue
dimension on the assembly preview. Diameter, shaft diameter, overall length, length
below holder, shoulder, flute and gauge dimensions use the entered units. The
preview stays visible while editing on a phone and can still be orbited or zoomed.

## Geometry selection and roughing

Use **2.5D milling → Recognize** to list or pick a model face, then recognize
features on that plane. The dock stays open while a feature is highlighted. A
matching operation uses the selected feature; its Geometry tab can choose other
recognized targets. Side/tilted faces use **Align face to +Z & fit stock** for an
explicit new setup. Existing linked operations then require rebuilding.

**3D roughing** uses successive constant-Z offset contours and stays down where
protected free regions connect. It is conventional clearing, without adaptive
engagement control. **Face** derives its target from the model and cannot cut below
its highest surface. Add top allowance if the model already touches blank top.

**New tool** appends a separate definition through a type chooser. Tool-table search
can be combined with the cutting-tool type filter. Live playback has a **Detail**
selector up to 512 cells; its Quick setting stays at 32 cells. Detailed inspection
also offers desktop detail up to 512 cells. Higher settings use more memory and
calculation time. [Workflow and implementation limits](MACHINING_WORKFLOW.md).

First launch opens the process chooser with an empty workspace. Saved projects
are offered through Continue workspace; the tray demo loads only from File →
Tray demo. Project properties are opened explicitly from the File menu.

## Imported-vise clamping profiles

Import a vise reference, then open Setup → Workholding → Configure clamping faces.
Pick the fixed gripping face, then the movable gripping face. Enter the measured
maximum closing travel and save the configuration. Inspect the highlighted movable
body before clamping. Only that connected body moves; disconnected inserts must
be included in prepared jaw geometry.

Set stock gripping depth and choose Clamp stock. The viewport shows raw stock and
animates geometric jaw closure; Back to vise setup returns to the controls. Open
jaw restores its configured opening. Closing updates the actual fixture mesh used
by tool/shaft/holder clearance checks, invalidating previous paths.

This first version accepts opposing flat rectangular faces along work X or Y and
unrotated rectangular stock. It rejects fused bodies, incompatible faces, stock
that does not fit, excess travel and final stock interference. It positions the
vise around the stock, leaving the part coordinates unchanged. Supports/parallels,
clamping force and internal vise self-clearances remain a manual review. It is not
a machine motion command or a physical holding-force solver.

Save to vise library stores a reusable profile in IndexedDB on this device. Existing text-storage library entries are migrated without discarding them. Export vise profile
creates portable JSON; Vise library can import it or add saved profiles to a setup.
Library profiles export with the jaw open and retain their source geometry and
work-coordinate placement; they do not configure a machine or controller.
