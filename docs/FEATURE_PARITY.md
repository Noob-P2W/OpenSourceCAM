# Native / browser feature coverage

Native desktop development and automatic package builds are paused. The browser
PWA and shop-hosting packages are the active development targets. Existing native
source and instructions are retained; their presence does not imply current
native release validation. See [browser development](PWA.md).

This is an implementation inventory, not a release promise or certification.
“Shared” means the same underlying Rust logic; device verification is separate.

| Capability | Native | Browser |
| --- | --- | --- |
| Project JSON and migrations | Supported | Shared loader |
| Bounded STL/OBJ import | Supported | Shared parsers; new fitted project |
| STEP/IGES import | Bundled runtime | Bundled OCCT WASM; tessellated mesh, source retained |
| Supported milling toolpaths and export preflight | Supported | Shared Rust core |
| Generate/export/simulate/plan command boundary | CLI --command | WASM worker |
| Stock simulation | Numeric and rendered | Shared sampled mesh and numeric report, 32–512-cell incremental playback with changed-strip transfers, selectable calculation budget, synchronized library tool assembly, and OBJ download |
| Geometry display | Native solid/mesh renderer | Depth-tested WebGL solids, stock volume, feature outlines, orbit/pan/zoom, axonometric default, top/front/right/isometric, grid/axes controls and fit; sampled software fallback |
| Machine/tool setup | Detailed native editors | Searchable machine/tool tables, persistent portable machine+post records, Generic/GRBL/Mach3/LinuxCNC templates, CNC profile import/export, configuration/tool changer, tool geometry/presets and assembly settings |
| Process opening menu | Nine process workspaces | Same Rust catalog and saved process identity; planned engines remain disabled |
| Tool library | 37 kinds and assembly preview | Same catalog, assembly geometry and focused dimension arrows (mm/in); separate tool creation, search and type filtering, editing, duplication, guarded removal, preset duplication/removal, inch display/editing, merge/replace import and JSON export |
| Planning controls | Blank allowances and operation replacement | Same PlanOptions in the worker; notes, XY/top/backing, 2.5D choices and explicit replacement |
| Detailed operation editor | All implemented settings | Dedicated operation drafts with geometry/tool/pass/feed/height tabs, labeled managed/inherited reference values, hover/focus/touch help, atomic validation/save, geometry unlinking and keyboard/context actions |
| NC inspection | Preview and export | Checked code preview and download |
| Stock fitting and workholding | Supported | Modal Setup: rectangular/round physical stock, XYZ stock/part orientation and offsets, shape-preserving fit, configurable example vise/backing and imported fixture transforms |
| Feature recognition | Selected stored plane respected | Face list/camera picking, explicit side-face setup, recognition at the selected XY level, linked operation targets, persistent candidate list and on-model highlights; unsupported/open-mesh diagnostics |
| Editing / undo history | Native implementation | Rust Session, bounded undo/redo and worker cancellation that preserves both stacks |
| Autosave / recovery | Native save protections | Local current/previous autosave and draft recovery |
| Install/offline updates | Native packages | Service worker, version display and update prompt |
| Controller streaming | Not implemented | Not implemented |

Verification currently includes Rust regression suites and Chromium phone-sized
and tablet-sized browser checks. These do not establish physical iOS/Android
coverage, complete collision verification, or machining accuracy. Record real
hardware reports with browser/OS versions and the visible PWA build id.

## Remaining UI differences

The main desktop milling workflows now have browser controls: process selection,
solid navigation, setup, tooling, planning, detailed operations, contextual actions,
playback, simulation, NC preview/export and cancellation. Desktop cancellation is
cooperative; browser cancellation restarts its worker and restores the project and
history from a bounded checkpoint. Current geometry is referenced, older retained
meshes are deduplicated, and the restored base geometry is checked. The desktop
selection panel is resizable on wide screens. Desktop/tablet use a ribbon and
project tree; phones use a tree drawer and responsive settings windows.

This does not establish complete device/UI equivalence. Native and browser history
budgets/edit grouping differ, native filesystem dialogs are browser downloads and
pickers, and real mobile graphics/storage limits still require device verification.
No additional process engine or recognition strategy is enabled by these UI changes.

The uploaded cartridge-cap STL was used for local browser regression checks only;
it is not included in the repository or deployed site. CI generates an original
closed cap fixture in viewport-smoke.mjs, and can optionally use CAM_TEST_STL for
local reproduction. The tests verify actual solid/stock pixels, camera gestures,
fitted stock dimensions, planning, overrides, sampled simulation, NC preview and
saved-project reload on phone/tablet viewports. This does not certify machining
that model's interior or undercuts in one setup.

Toolpath playback keeps fractional progress independently of the slider and pauses
when scrubbing, leaving the workspace, changing the project or starting a calculation.
Pause/resume retains position; Play after completion replays from the start. The
contrasting position marker interpolates between supplied move endpoints in WebGL
and software rendering. By default the browser draws the library cutter, shaft
and configured holder at this position, switching with the operation's tool.
Show tool can be disabled to return to the position marker. Assembly meshes are
illustrative library geometry, not measured cutter flute surfaces or additional
collision verification. Playback uses the shared Rust estimated-time timeline and the selected speed multiplier.
With Simulated stock enabled, bounded worker snapshots (64 cells by default, up to 512 selectable) update during
playback; rewinding immediately restores the uncut mesh. The detailed inspection
command retains selectable resolution, target comparison and fixture reporting.
The browser regression runs playback at normal frame intervals, checks visible
movement within a segment and exercises the player on phone/tablet/desktop layouts.

Standard drilling and full-retract peck drilling share expanded G0/G1 paths across
Generic, GRBL, Mach3 and LinuxCNC. Per-revolution library drilling feeds follow
spindle overrides. Library drilling requires a drill definition. Outside profiles
and stock clearing now use circumscribed rounded corner paths; inward profiles
and raster reversals retain radius/allowance compensation. See [drilling and
compensation](DRILLING_AND_COMPENSATION.md) for exact behavior and primary sources.

3D roughing now uses top-down constant-Z offset clearing with stay-down links in
connected free regions. It remains conventional clearing rather than adaptive
engagement control. Facing stops at the highest imported target surface; lower
recessed levels require a suitable strategy. Recognized operations retain the feature opening and clear stock above it in
bounded depth passes; earlier whole-stock facing can establish a cleared starting
level. See [machining geometry and clearing](MACHINING_WORKFLOW.md).

## Machining workflow update

Browser playback uses the Rust feed/rapid timeline: 1× is estimated elapsed machining time, excluding controller acceleration and tool-change delays. Face opens direct model picking and returns to the same operation draft. Pocket/hole drafts can bind picked geometry before choosing a tool; generation retains cutter-fit checks. The separate Setup → Auto recognize survey groups opposite views of a through-hole and inspects supported prismatic openings across all planar levels and opening-face orientations (bounded to 64 orientations / four million triangle-orientation visits / 500 candidates). It changes neither setup nor operations. Other directions require explicit setup orientation and fixture review; there is no automatic workflow recommendation. Right-click inspects current-axis holes/pockets or a model face. Selected geometry has an orange surface shade and outline.

Finishing and roughing contact queries use a bounded triangle hierarchy with analytic swept-flat-cutter contact and conservative ball-cutter subdivision rather than a fine whole-stock height grid. Finishing uses adaptive parallel spans, with separate sampling (0.005–5 mm), contact tolerance and stepover controls. Roughing still uses a bounded 262,144-cell clearing grid. The 50-million-work and 200,000-move safeguards remain; fine sampling is not a guaranteed global surface-error bound. Mesh tessellation and cutter/stepover still constrain finish. Simulation resolution remains separate from machining accuracy.

Example vise seating is edited in Setup → Workholding → Reseat stock / edit jaw grip. The diagram distinguishes jaw grip from backing material; reserving backing can grow blank thickness. Imported fixtures retain measured offsets and rotations.

## Feature tree, selection and manual changes

Browser recognition now lives under the part in Project & operations; the right-side recognition dock has been removed. Recognized holes have selectable markers, with dimensions in their labels/tooltips. Left-click inspects/selects; right-drag pans and never performs selection. Actual model triangle indices supply selection shading, including chamfer/fillet surfaces; no extruded substitute walls/floors are displayed.

The detailed survey recognizes bore entry chamfers, rounded transitions, tapered holes/pockets, islands and open pockets/ledges. Taper, island and open-pocket results are inspection candidates, not permission to flatten them with a simple rectangular pocket/drill path. Compatible bores can populate operation geometry before selecting the tool. Legacy stock planning still uses its conservative prismatic recognizer. Recognition remains bounded; arbitrary intersecting/freeform geometry is not classified exhaustively.

Path lines can be hidden globally or per operation without hiding the moving cutter or changing generated paths. Camera navigation does not pause playback. When sampled stock is visible, the tool and displayed clock use the same processed snapshot time, so a delayed high-resolution stock calculation cannot leave the cutter ahead of the removed material. The tool HUD distinguishes cutting diameter from a larger shaft. Sampling error remains bounded by the preview cell size.

Visibility buttons use eye icons with pressed state. Linked geometry fields show
“Managed automatically” and their reference dimensions inside the box; empty cutting
overrides show resolved inherited values. Field help supports mouse hover, keyboard
focus and touch editing. These presentation controls are browser features.

Multi-tool generation/simulation no longer depends on ATC export configuration. Export retains geometry/fixture preflight and additionally checks the configured manual-change position and calibrated H registers, or the existing ATC configuration. Generic/Mach3 support mandatory M0 changes; LinuxCNC uses its configured manual M6 handshake. GRBL remains one tool per NC file. See [manual tool changes](MANUAL_TOOL_CHANGES.md).

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

Fixture display preserves complete reference meshes and indexed vertex sharing.
It does not decimate by deleting faces. This applies to WebGL and software fixture
surfaces, within the existing workspace limits. It does not add vise kinematics
or certify physical clamping, imported fixture dimensions, or collision geometry.

Imported-vise profiles use two picked gripping faces, a measured closing-travel
limit and a movable rigid group. Browser STEP/IGES imports retain each source
solid/shell mesh as a named component, including its assembly path when supplied
by the kernel. Vertex sharing and planar-face grouping stay within those component
boundaries, so touching parts are not fused. The setup lists the components and
highlights each one; the movable gripping component is mandatory and additional
inserts/fasteners can be attached to its rigid group. Other components keep their
imported relative positions. The group has one work X/Y translation with a travel
limit; imported CAD mates and general assembly constraint solving are not supported.
STL/OBJ, older saved imports and the paused native CAD adapter have no source
component identities and retain the exact-connectivity fallback; touching mesh
shells may be indistinguishable. Reimport a browser STEP assembly to obtain its
component metadata. The supported contact remains unrotated rectangular stock,
opposing rectangular planes, and straight jaws with their moving geometry behind
the gripping plane. Fused solids, stepped/curved contacts and other orientations
still require prepared geometry. A flattened vise exported as one connected CAD
body cannot provide independent jaw motion. The clamping panel identifies a
single-component import and asks for a separate-body assembly export.
Imported vises default to “Move stock and part into fixed vise” in the browser.
The built-in example keeps its existing “Move vise around stock” default. The
placement choice remains available before clamping; picking faces alone records
a draft and prompts the user to clamp.

The shared Rust `ClampStockInVise` / `ClampStockInViseSetup` Session actions seat
stock and part in a fixed vise by relocating `machine.work_origin`. They preserve
stock, part and operation machining coordinates and geometry links, pin the vise
in `Frame::Machine` at its original machine offset, and materialize the closed jaw
mesh. A machine definition is required. Other machine-framed fixtures retain their
physical placement; stock interference is checked again against the final fixture
poses. Setup, closure and origin changes roll back together on failure. The legacy
`ClampVise` / `ClampViseSetup` actions place the vise around the stock instead.
SAT stock/fixture surface interference and closed-shell stock
containment are checked atomically. The same posed mesh feeds existing tool/shaft/
holder clearance preflight; edits invalidate paths. Jaw interpolation is display
only. Internal vise self-clearance, supports/parallels and physical clamping forces
are not simulated. Vise picking, profile configuration, queries and opening support
both work and machine frames. Opening restores jaw geometry without moving the
work origin or fixed vise; reclamping can reseat stock at another grip. Existing
machine coordinate limits and NC travel/clearance preflight remain enforced.
Profiles persist through project saves, undo and worker recovery. Device-local IndexedDB
vise library records and portable JSON profiles reuse the prepared model and
configuration. Library export restores the jaw's open geometry and converts its
placement to portable work coordinates, so reuse needs no source machine. No machine
connection or calibrated controller binding is created.

Model imports accept at most 250,000 triangles / 750,000 imported vertices. CAD
tessellation shares exactly equal vertex coordinates within each source component;
it does not weld nearby points, discard faces, or relax the existing 0.1 mm
linear / 0.3 rad angular tessellation settings. The 32 MiB source file limit,
16-model / 300,000-total-reference-triangle workspace budget and simulation mesh
budgets remain separate and unchanged. Large supported inputs still need device
verification; exceeding the new cap fails before changing the workspace.

Reference assemblies now have an expandable component list below Workholding in
the Project & operations browser. Clicking a component recolours its existing
render buffer instead of drawing a coplanar duplicate. Clamping controls are
non-modal; moving-group checkboxes live in the component tree. Selection and
reference framing retain the complete source geometry. Import and scene selection
frame the reference itself rather than including a distant stock origin. The
viewport includes a scene-sized grid and a camera-oriented XYZ indicator.

Shared Session placement transactions support reference XYZ offsets, XYZ rotation,
work/machine frames and XY centering over stock. Individual CAD components can
be positioned by their center in work millimetres and rotated by explicit XYZ
increments about that center. These edits bake into the geometry consumed by
clearance checks and survive undo/save/recovery; moving a component clears its
clamping profile, and closed jaws must be opened before repositioning. Changing
assembly orientation/frame also clears the profile. This is positioning and rigid
jaw grouping, not a general CAD mate solver or internal vise mechanism simulation.
Clamping remains limited to the documented parallel-jaw geometry and stock cases.
The clamp panel and travel errors report opening, stock span, required closure and
the configured travel allowance; face queries inspect only the selected source
component instead of grouping all triangles in a detailed vise repeatedly.

Clamp stock applies the current clamping draft and closes in one atomic Session
action, without requiring a separate Save click. Missing selections and rejected
travel produce visible errors; an already fitting blank reports zero jaw travel.
Scene centering ignores unused source vertices, matching the rendered mesh.

## Three.js viewer and performance

The WebGL path now uses packaged Three.js BufferGeometry, an orthographic camera,
per-component scene objects and cached indexed meshes. The pinned r162 dependency
preserves WebGL 1 support as well as WebGL 2; its MIT license is included under
`vendor/three-LICENSE.txt`. Both the module and license are cached offline. The
software fallback, touch gestures, face selection, tool dimension preview, stock
playback, fixture selection, clipping-free reference framing and graphics recovery
remain covered by browser tests. Scene projection is exposed through the viewport
projection method rather than tests depending on custom shader uniforms.

Raw geometry keys and transforms come from Rust. Rigid positioning updates object
matrices, while exact-shape caching can share repeated component buffers. GPU
positions are relative to a local mesh origin for precision at distant placements.
Cache lookups verify geometry data before deduplicating hash buckets; cached unused
meshes have a 64 MiB budget and geometry/material resources are disposed explicitly.
Display normals remain flat across triangle boundaries without duplicating every
vertex; devices lacking the relevant WebGL extensions use expanded geometry.
No CAM geometry, tolerance, import limit or export check is reduced for rendering.

The Rust browser adapter caches current reference previews and geometry keys by
shared Geometry ownership plus placement, and caches the active part fingerprint.
Caches retain current models only, and machine-frame previews invalidate when the
work origin changes. Face picking transforms shared vertices once per query.
Stock playback reuses Rust-provided interleaved position/normal arrays, updates
only dirty strips and retains model/tool buffers; line geometry drops unused normal
fields on the WebGL path. The axis widget and drawing surface resize only when
the relevant view state changes. These are scoped optimizations, not a claim that
every CAM algorithm has been replaced or that arbitrary assemblies are certified.

Imported reference placement matrices are also applied in the software renderer.
Both WebGL and software pose tests check translated/rotated vise pixels and jaw
opening/closure. Fixed-vise clamping keeps the fixed jaw anchored on screen while
stock moves, with picking still expressed in work coordinates. Explicit placement
and Fit reset the temporary camera anchor.
