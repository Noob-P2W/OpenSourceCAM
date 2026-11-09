# Current architecture

OpenSourceCAM is one Rust package. Modules provide boundaries; there is no enforced
multi-crate layer graph yet. The browser uses a Rust editing session; the native desktop history remains separate.

| Layer | Files | Responsibility |
| --- | --- | --- |
| Domain | model, operation, machine, tool_library, process, workspace | Shared project data and validation |
| Geometry and computation | import, features, collision, machining, surface, stock, cam | Bounded import, planning and sampled simulation |
| Output | post, files | Controller formats, preflight and native atomic file writes |
| Application calculations | commands | Generate, export, simulate, stock/surface planning; errors returned without dialogs |
| Editing session | session | Atomic draft replacement, operation management, bounded undo/redo |
| Native adapter | main, ui, mesh_view | CLI, desktop editing and graphics |
| Browser adapter | web.rs, web/worker.js | WASM transport to shared application commands |
| Browser presentation | web/app.js, viewport.js, index.html, style.css | Touch-oriented controls, workspace persistence, file downloads |

Both `--command` in the CLI and the browser dispatch through `commands::execute`.
Command ids are generate, export, simulate, recognize, plan and surface. For example:

```sh
cargo run --locked --no-default-features -- --command generate examples/tray.cam.json
```

The output is JSON; export returns a JSON string containing G-code. Existing CLI
commands retain their original formats. Calculation commands do not mutate input
projects; planning returns a new project. Native desktop actions still call the
same underlying Rust functions directly. Browser edits and undo/redo now run through session::Session. Native desktop
history and edit grouping have not yet migrated. The session API is available
in native builds for that subsequent integration.

The browser keeps the Rust engine in a worker. File dialogs/downloads and local
storage remain platform responsibilities. Native STEP/IGES needs the bundled
Python/OCP runtime. Browser CAD runs in a disposable OpenCascade WASM worker;
its output passes Rust Part/Project validation. STL/OBJ use shared Rust parsers.

## Quality and deployment

After installing the prerequisites documented in PWA.md / NATIVE_GUIDE.md:

- `python3 scripts/check.py native`: formatting, desktop Clippy/tests, headless tests.
- `python3 scripts/check.py web`: formatting, web-feature Clippy/tests, WASM build,
  packaging, smoke tests of the packaged site, update/recovery browser tests.
- `python3 scripts/check.py all`: both gates, stopping on failed commands.

Browser tests require `npm ci` in web and Playwright Chromium (set CHROMIUM_PATH
when it is not /usr/bin/chromium). Windows native checks need Python available as
python3. The web build script uses Bash. Neither gate installs OS/browser packages.
The native CI additionally builds releases and tests bundled CAD import. Core gate
success does not imply those CAD tests ran.

Successful main web builds update gh-pages and explicitly request a Pages build.
GitHub must be configured to serve gh-pages / root. Local tests do not establish
that the remote CI passed or the website finished publishing.

## Draft editing

Session edits construct a candidate before replacing the current project. Failed
commands leave project and history unchanged. New successful edits invalidate redo.
History is bounded to 30 entries / 8 MiB of retained metadata/geometry estimates and resets on reload.
Large snapshots can exhaust the history budget; autosave and downloaded backups
remain separate protections. A typed draft may temporarily contain invalid machining
values so it can be repaired; generation/export still use the full existing validation.
The browser keeps only a presentation copy of the committed session view. Its worker
owns the session and calculates against that state; JavaScript has no undo stack.

Setup actions (stock fitting, CNC profile application, tool-library import and
reference-model insertion) and candidate-feature operation creation are Session
transactions. Recognition and feature-to-operation conversion use shared Rust
functions. The browser renders reference meshes from Rust-transformed vertices;
no independent fixture coordinate calculation is used in JavaScript.

## Large-model memory and rendering

STL import interns exactly equal vertex coordinates; it retains every triangle and
its winding, with no geometric decimation. Import commits directly inside the
worker's Rust session rather than sending a project to the page and back again.
Calculation commands borrow the session project instead of serializing/reparsing it.
Undo snapshots clone Project metadata and share immutable Geometry through Arc;
unchanged geometry from browser metadata replacements is reattached to the same Arc.
The history budget excludes geometry shared with the current project, and includes
unique older meshes retained by undo. Browser views serialize borrowed project data
without an intermediate serde_json::Value representation of the whole mesh.

The page caches compact project JSON per committed revision; the advanced editor
is populated lazily for large projects. Canvas redraws are coalesced, geometry
paths cached/batched, backing buffers bounded to 4 million pixels and DPR 2, and
main workpiece solids rendered at full resolution in WebGL. Wireframe/software
fallback previews sample large meshes; reference model previews retain the
12,000-triangle worker budget. The camera fits the complete stock/part volume,
including negative Z; orbit, zoom and pan are display state only.
This limit is presentation-only and is disclosed in the preview; full geometry
remains available to validation, planning, simulation and project downloads.

## Restored browser workflow

Process and tool editor catalogs come from Rust definitions, including all nine
processes and 37 library kinds. Shared Session transactions create process-specific
projects, retain process identity when importing a part, add typed strategies and
manage library tools/presets. Planned motion engines remain disabled. Planner
options deserialize into machining::PlanOptions; commands::plan_project dispatches
to the native planners. Browser planning commits inside the worker and returns
notes with the new Session view, avoiding a large project round trip.

Illustrative tool/holder geometry was extracted from the native library renderer
into tool_preview::assembly. Both UIs consume that helper; it is not collision
geometry. Browser stock simulation renders the actual mesh from stock::simulate,
with a display toggle and sampling/detail control. Operation path indices support
selected-operation filtering without changing generated/postprocessed paths.

## Browser cancellation and graphics recovery

A long browser job first obtains a Session history checkpoint. Its current meshes
are references to the already-cached committed project, not additional serialized
copies. Unique older geometry is stored once; project metadata references that pool.
Cancellation terminates the worker, restores the committed project and both history
stacks in a new worker, checks base-geometry fingerprints, and resumes editing.
Restoration returns history flags rather than another large presentation copy.
Checkpoint parsing remains size-bounded and rejects invalid geometry references.

The viewport handles WebGL context loss and rebuilds its GPU resources on restore.
Software rendering remains available when WebGL is unavailable. Camera movement,
selected-path filtering, inspector resizing and playback do not create project edits.
Model-link fingerprints canonicalize signed zero, matching browser JSON number
semantics. Older links containing negative-zero hashes may need rebuilding; export
validation is retained.

Explicit operation replacement validates a planning input with the discarded old
operations removed. This lets stock fitting or model-placement changes be repaired
by rebuilding, while generation/export continue to reject stale geometry links.
The resulting new operations still pass the shared planner/path validation.

Browser field edits update the presentation draft and are committed together after
focus leaves the fields. Every action flushes that draft first. This prevents a
field's blur handler from disabling and swallowing the button tap that follows it.
During the commit, the matching presentation object is retained so queued control
closures keep valid references; history flags come from Rust, and the GPU scene is
invalidated after geometry/setup changes. Invalid commits roll back atomically.
