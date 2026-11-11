# Current architecture

OpenSourceCAM is one Rust package. Modules provide boundaries; there is no enforced
multi-crate layer graph yet. The browser uses a Rust editing session; the native desktop history remains separate.

| Layer | Files | Responsibility |
| --- | --- | --- |
| Domain | model, operation, machine, machine_library, tool_library, process, workspace | Shared project data and validation |
| Geometry and computation | blank, workholding, import, features, collision, machining, surface, stock, cam | Bounded import, planning and sampled simulation |
| Output | post, files | Controller formats, preflight and native atomic file writes |
| Application calculations | commands | Generate, export, simulate, stock/surface planning; errors returned without dialogs |
| Editing session | session | Atomic draft replacement, operation management, bounded undo/redo |
| Native adapter | main, ui, mesh_view | CLI, desktop editing and graphics |
| Browser adapter | web.rs, web/worker.js | WASM transport to shared application commands |
| Browser presentation | web/app.js, shell.js, viewport.js, index.html, style.css | Touch-oriented controls, workspace persistence, file downloads |

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
Routine CI focuses on the browser gate plus headless Rust formatting, Clippy and
tests with `--no-default-features`. Native desktop development and automatic
release packaging are paused. The optional native job only runs on manual
workflow dispatch with `build_native` enabled; it retains release and bundled CAD
checks. Routine core/browser success does not imply native package validation.

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

STL import interns exactly equal vertex coordinates; it retains every nondegenerate triangle and
its winding, with no geometric decimation. Zero-area import cleanup is reported below. Import commits directly inside the
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
fallback previews sample large meshes; reference model previews preserve every face and shared vertex within the existing
16-model / 300,000-reference-triangle workspace limits. Software fallback also
retains complete fixture surfaces; its workpiece/stock display remains sampled. The camera fits the complete stock/part volume,
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

## Manufacturing shell and physical blanks

The browser shell owns command presentation, modal drafts and software preferences.
Its operation Save and setup actions are Rust Session transactions. MachineRecord
normalization and portable import validation also run in Rust. Custom machine records
and explicit tool device libraries use browser storage; project state retains its
applied machine/post and tool definitions. Cancellation checkpoints retain part
rotation and configurable vise metadata.

Project format 10 introduced part rotation and StockProfile. Existing axis-aligned stock
fields retain the work-axis envelope. StockDefinition records physical block sizes
or cylinder diameter/length plus XYZ orientation. Rust supplies world matrices and
blank outlines to the viewport. Planning containment and stock simulation use the
physical solid; the JavaScript renderer only applies supplied display matrices.
FitStock preserves the selected profile. PlanOptions.fit_stock defaults true for
legacy callers; the browser explicitly sends false when using its measured Setup.
Zero part rotation preserves prior geometry-link hashes.

The example vise is original generated geometry, stored as a workspace reference
with its ViseSpec. Imported references keep their existing placement controls.
Native display and shared calculations understand these project fields; the new
modal/ribbon workflow and stock definition editor are browser features.

## Stock playback and drilling

Project format 11 adds standard single-feed drilling and optional peck re-entry
clearance. Older `drill` operations retain their peck identity; legacy project
migrations preserve existing geometry-link identifiers. All cycle motion and
cutter compensation remain in Rust. Circular outside corners use a circumscribed
tangent polygon bounded by chord tolerance, avoiding the gouge of inscribed chords.

Generated path presentation includes Rust-computed start/end seconds for each
move. Both the cursor and stock snapshots select the same estimated-time prefix.
WebSession caches one preflighted PlaybackPreview path, invalidating it on every
successful edit/import/plan. A generate response includes its bounded uncut stock
mesh. The browser requests stock updates paced by animation frames (64 cells by default), with
one request in flight and only the latest target queued. Rust retains the sampled
heightfield and advances only newly elapsed motion. Tight swept-capsule row bounds
avoid scanning diagonal bounding rectangles; already cleared cells skip cutter
contact evaluation. Changed eight-row mesh strips travel as transferable Float32
buffers. The renderer replaces those strips and retains other GPU buffers. Epoch
and revision checks reset/discard stale state after seeking or editing. These updates share stock
removal, omitting repeated target recognition/collision reports;
path preflight and full inspection keep those checks. Browser code does not perform
stock removal. The detailed inspection/export route remains independently checked.

Application confirmations use themed HTML dialogs with cancel/Escape and focus
restoration. The browser's own unsaved-page departure protection remains controlled
by the browser. Tool-preview dimension anchors and values come from Rust in the
assembly coordinate frame; SVG overlays project those points using the display
camera. Focusing a dimension selects a front view, and the drawing stays visible
while editing on small screens.

Playback resolves the effective operation tool in Rust and reuses the library's
tip-at-zero assembly meshes, including the configured shaft and holder. The page
keeps at most four assemblies, requests one at a time and preloads the next
distinct tool. Only the current tool occupies GPU buffers; stock updates do not
rebuild those buffers. A display translation places its tip at the shared cursor,
with depth-tested solids in WebGL and depth-sorted faces in software fallback.
The camera includes a retained swept tool envelope rather than following each
move. Legacy jobs use their setup dimensions and are labeled as legacy tools.
Display geometry does not alter collision envelopes, removal or posted tool offsets.

## Geometry targets and offset clearing

Project format 12 records the selected setup face and operation-specific machining
face/opening depth. Legacy projects migrate with the highest XY plane and zero
opening depth. Geometry-link hashes include explicit face/opening selections while
preserving older hashes when these fields are absent. Checkpoints retain the setup
face through undo/redo and cancellation.

`faces` groups connected coplanar triangles and resolves camera-ray selection in
Rust. `commands::geometry_query` supplies face lists, selected outlines and linked
operation drafts. Selecting a side face is an explicit Session transaction that
aligns its normal to +Z and fits a new blank. Recognition uses the selected level;
features obstructed above it are rejected. Opening depth is geometric metadata;
only a preceding whole-stock facing pass proves that higher stock can be skipped. Native recognition also respects a
stored setup plane. Browser recognition is a responsive nonmodal dock so the list
and actual highlighted workpiece can be inspected together.

`roughing` constructs conservative free regions from triangle-hierarchy cutter contact and
clears offset contours at successive tool step-downs. Eroded grid shells bound
spacing in both axes; routing links only traverse protected free-cell edges.
Connected regions stay at Z; separate components retract. Surface finishing retains
the separate parallel path. Facing is limited to the highest target surface.

Detailed stock inspection and explicitly selected live previews support up to 512
cells per side. High-detail generated meshes have a separate bounded triangle
budget; input mesh limits remain unchanged. Browser preferences select 50, 100 or
200 million expensive removal evaluations per update/inspection. Auto selects
100 million when the browser reports at least 4 GiB device memory and four logical
processors, otherwise 50 million. This is a capacity hint, not a free-memory probe.
A separate traversal cap is four times the selected evaluation budget. Native
default calculations retain 50 million. See
[MACHINING_WORKFLOW.md](MACHINING_WORKFLOW.md) for scope and limits.

Surface contact now uses `contact.rs`: a triangle bounding hierarchy, analytic capsule/triangle contact for flat cutters, clipped local patches, conservative ball-cutter contact bounds and cancellable subdivision. `surface.rs` makes adaptive finishing spans; `roughing.rs` retains its separate bounded clearing grid. Browser Setup surveys use `GeometryQuery::Survey`, while contextual plane recognition remains available to legacy planning/native callers. Geometry-only feature drafts resolve selections against Rust recognition and defer cutter-fit validation until tooling can be edited.

Detailed feature surveys augment conservative prismatic planning with connected wall-profile chains and recessed floor regions. `Feature.detail.triangles` refers to the original mesh ordering for exact surface highlighting; profile/island metadata supports inspection without enabling unsupported simple machining. Browser markers and the part tree share a single selection and pending operation draft.

`post::checked_geometry_path` is the shared planning/simulation preflight; `checked_path` adds export post/tool-change validation. Manual-change approach/return routes receive travel and fixture checks in addition to the cutting path. Export emits calibrated compensation and controller-specific operator stops; controller-managed M6 motion remains the controller's responsibility.

Browser stock simulation requires current generated, nonempty toolpaths. Editing
invalidates them and disables stock simulation until generation succeeds. Inspection
and playback share one resolution control; updates have no fixed timer/FPS cap,
with one worker request in flight and only the latest target queued.

New STL/OBJ model imports and CAD tessellations automatically discard zero-area
triangles and orphan vertices, preserving valid face coordinates and winding without a welding
tolerance. Removed counts are retained in geometry metadata and shown in the
browser import status and Setup. Input limits, coordinate/index checks and strict
project validation remain. All-degenerate meshes fail without replacing the
workspace. Cleanup does not fill holes or certify manifold/closed geometry;
existing saved projects are not automatically rewritten because face links may exist.
