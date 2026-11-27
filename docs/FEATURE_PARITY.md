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
| Supported milling toolpaths and export preflight | Shared generators; native UI development paused | Shared Rust core; grouped 2D operations, model/numeric chains and checked export; see [2D milling limits](MILLING2D.md) |
| Generate/export/simulate/plan command boundary | CLI --command | WASM worker |
| Stock simulation | Numeric and rendered | Shared sampled mesh and numeric report, 32–1024-cell incremental playback with changed-strip transfers, selectable calculation budget, synchronized library tool assembly, and OBJ download |
| Geometry display | Native solid/mesh renderer | Depth-tested WebGL solids, stock volume, feature outlines, orbit/pan/zoom, axonometric default, top/front/right/isometric, grid/axes controls and fit; complete-triangle software solid fallback |
| Machine/tool setup | Detailed native editors | Searchable machine/tool tables, persistent portable machine+post records, Generic/GRBL/Mach3/LinuxCNC templates, CNC profile import/export, configuration/tool changer, tool geometry/presets and assembly settings |
| Process opening menu | Nine process workspaces | Same Rust catalog and saved process identity; planned engines remain disabled |
| Tool library | 37 kinds and assembly preview | Same catalog, assembly geometry and focused dimension arrows (mm/in); separate tool creation, search and type filtering, editing, duplication, guarded removal, preset duplication/removal, inch display/editing, merge/replace import and JSON export |
| Planning controls | Blank allowances and operation replacement | Same PlanOptions in the worker; notes, XY/top/backing, 2.5D choices and explicit replacement |
| Detailed operation editor | All implemented settings | Dedicated operation drafts with geometry/tool/pass/feed/height tabs, labeled managed/inherited reference values, hover/focus/touch help, atomic validation/save, geometry unlinking and keyboard/context actions |
| NC inspection | Preview and export | Checked code preview and download |
| Stock fitting and workholding | Supported | Modal Setup: rectangular/round physical stock, XYZ stock/part orientation and offsets, shape-preserving fit, configurable example vise/backing and imported fixture transforms |
| Feature recognition | Selected stored plane respected | Face list/camera picking, explicit side-face setup, recognition at the selected XY level, linked operation targets, persistent candidate list and on-model highlights; unsupported/open-mesh diagnostics |
| Editing / undo history | Native implementation | Rust Session, bounded undo/redo and worker cancellation that preserves both stacks |
| Autosave / recovery | Native save protections | IndexedDB current/previous autosave, legacy migration and draft recovery |
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
connected free regions. It uses conventional offset clearing. Parallel finishing
limits each row to the model's projected cutter reach and combines continuous
flat spans. Short entries use remaining-stock, tool, holder and fixture checks.
Where an entry cannot be certified, the planner extends a pass to the stock edge
or continues from the preceding certified entry. Facing stops at the highest imported target surface; lower
recessed levels require a suitable strategy. Recognized operations retain the feature opening and clear stock above it in
bounded depth passes; earlier whole-stock facing can establish a cleared starting
level. See [machining geometry and clearing](MACHINING_WORKFLOW.md).

## Machining workflow update

Browser playback uses the Rust feed/rapid timeline: 1× is estimated elapsed machining time, excluding controller acceleration and tool-change delays. Face opens direct model picking and returns to the same operation draft. Pocket/hole drafts can bind picked geometry before choosing a tool; generation retains cutter-fit checks. The separate Setup → Auto recognize survey groups opposite views of a through-hole and inspects supported prismatic openings across all planar levels and opening-face orientations (bounded to 64 orientations / four million triangle-orientation visits / 500 candidates). It changes neither setup nor operations. Other directions require explicit setup orientation and fixture review; there is no automatic workflow recommendation. Left-click inspects current-axis holes/pockets or a model face. Selected geometry has an orange surface shade and outline.

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

Each reference has one main entry below Workholding. Right-click opens its
floating action menu; component membership is listed only in the clamping panel.
The placement editor's component selector retains individual-body positioning.
Selection and framing retain complete geometry, with no duplicate left-side list.
The viewport includes a scene-sized grid and camera-oriented XYZ indicator.

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

## Reference viewing, import order and leaving simulation

The WebGL camera depth range covers visible scene geometry independently of the
fitted selection, including translated/rotated references and moving jaws. Picking
starts in front of those references. The viewport **Center on vise** button beside the fit controls fits the selected
vise (or the first vise) and uses its center for orbit without
changing geometry or machining placement. Placement still has its separate
**Center over stock** action.

Workpiece import replaces the workpiece and fitted stock and clears its old
operations and selected setup face. It retains reference models, process identity,
CNC/post configuration, tool library and active tool/preset, whether the vise or
workpiece was imported first. Reference import also retains the current workpiece
and operations. Both are atomic, undoable session edits.

**Simulated stock** is available in the main viewport controls. **Turn simulation
off** pauses playback, clears the sampled stock display and pending preview queue,
and restores the original model. Completed results arriving after it is disabled
cannot restore simulation. Generated toolpaths remain available for playback or
export. The existing calculation-cancel control stops an in-progress explicit
simulation calculation.

The viewport places **Center on vise** beside **Fit part** and **Fit all**.
The Actions ribbon keeps **Stock simulation**; the duplicate Material removal
entry is removed. Its menu includes **Disable stock simulation**. Simulation
defaults to 128 cells with explicit 768/1024-cell options for larger stock;
existing calculation and work limits remain enforced.

## Project UI actions and setup

References, workpieces and tools/holders expose deletion from their project-list
action menus. All project entries and operations use a shared floating right-click action menu.
Keyboard users can press Shift+F10 or the menu key; touch users can long-press.
Menus close on selection, Escape or outside click and stay within the viewport.
Deletion is undoable; removing a workpiece also removes its model-linked operations,
while assigned tools/holders retain their existing assignment checks. Visibility,
operation enabling and jaw-group membership use native checkboxes. Imported-vise
setup lists moving components directly in Setup → Workholding → Set up clamping.
Setup has one Save setup and continue action; redundant Orientation and NC-preview
ribbon shortcuts were removed while their controls remain in the relevant dialogs.
The project sidebar supports mouse/touch resizing, keyboard width changes, header
dragging into a floating panel, and Dock; narrow screens retain the drawer.

The clamping panel has one action button: **Clamp stock** when released and
**Release stock** when clamped. It stays open and updates the text after each
successful action. The single button is in a fixed footer so it stays visible
when the component/placement settings are scrolled. The separate Open jaw button is removed. Clamped state is saved
explicitly, including zero-travel contact; legacy profiles with positive jaw travel
are recognized as clamped. Undo, reload and profile export retain correct state.

Machine selection retains the existing work origin, stock, part, fixture pose,
clamped state, work offset and tool assignments; it replaces the CNC definition
and postprocessor without importing another legacy cutter. The tree groups unique
used cutters separately from fixtures/vises. The workpiece branch has its own
expansion guide; duplicate Import/Setup/Operations shortcuts, the recognition
explanation and the second Scan button are removed. The workpiece context menu
can seat stock and part in the configured vise and clamp them.

Actions → Stock simulation and Post / export open separate dialogs. NC preview
and export contain no material-removal controls. Inspect provides distinct
axonometric (30° elevation) and isometric views plus an **Inspect part features**
checkbox. In inspection mode, selecting a supported hole/pocket, mesh surface or
surface boundary edge displays measurements in the right inspector. Surface area,
normal, inclination and boundary lengths come from Rust; mesh edge measurements
follow the imported tessellation and do not infer analytic CAD curves.
Middle-drag orbits, right-drag pans, and left-click selects; touch orbit/pinch
remain available. The cube's diamond is Home view, and TOP remains directly
clickable. The redundant view dropdown, Inspect fit buttons and permanent stock
size overlay are removed. Vise component hover highlights the corresponding
geometry; on desktop the setup dock fills the available right-side height.


Browser wheel zoom keeps the point under the pointer fixed after panning. Zoom
buttons use the viewport center, and pinch zoom follows the midpoint between the
fingers, including simultaneous pan. The existing zoom limits still apply.

Projects autosave asynchronously to IndexedDB after committed edits, including
imports, placement, clamping and undo/redo. A single transaction rotates the
current/previous snapshots, so an interrupted or failed save retains both earlier
snapshots. Existing localStorage projects/backups migrate without changing their
JSON; legacy text is removed only after the database commit. Invalid saved JSON
remains available through Recover draft. File → Backup & recovery → Previous
autosave restores the previous snapshot and remains undoable. The footer reports
Saving, Autosaved, or Not saved; updates await a completed save before reloading.
Pending field edits are flushed when the page becomes hidden when possible;
leaving with unsaved changes retains the browser unload warning. Small preferences
and device tool/machine libraries retain their existing storage. Offline packaging
includes the storage module. Browser storage may still be evicted or unavailable;
downloaded project backups remain independent of it.


Recognition survey anchors and outlines use the inverse setup rotation when
returning to work coordinates. Open-pocket/island markers use an actual floor
triangle rather than an elevated bounding-box center. Grouped through-holes keep
both opening anchors and display the one facing the camera. Part bounds are
reused across recognition level loops; existing recognition budgets and machining
validation remain unchanged.


Open-pocket/island inspection highlights complete the selected connected planar
faces so their edges follow physical surface boundaries rather than mesh diagonals.
This display query leaves recognized feature dimensions and machining geometry
unchanged. Results from a superseded selection are ignored.

Floating feature markers are visible only while Inspect → Part features is checked.

Grouped browser 2D operations now include rectangular/circular Pocket, planar chain
Contour, smooth trochoidal Roughing, cutter-width Slot, varying-Z Trace,
Bore/Circular/Thread wall milling, Engrave and Chamfer. These use shared Rust
generators. Smooth roughing is not constant-engagement adaptive clearing. Thread
operations require measured matching-pitch cutters and precleared stock; sampled
stock simulation is unavailable for enabled thread operations, while checked
paths and NC remain available. See [2D milling](MILLING2D.md) for geometry, entry
and safety limits. No sketch editor or arbitrary pocket/island solver is added.

Stock fitting preserves top/backing allowances separately from the current part-to-blank
gaps. Part rotation and repeated fitting no longer reinterpret empty space as extra
backing. The saved values survive project reload, undo and cancellation recovery.

Open-pocket recognition now follows upward concave wall patches from each floor
boundary, requiring complete bounded patches and a consistent rim height. It no
longer borrows depth or surface triangles from a connected wall component wrapping
the whole part. Ambiguous ledges with unbounded walls or inconsistent rims remain
unclassified. Feature shading uses the validated triangle set directly, without
expanding a selected fragment to an unrelated whole plane.

Browser finishing now includes connected machining-edge picking with hover previews,
zero/positive tool-tip offsets, and planar face smoothing with island clearance.
Chamfered straight slots have separate slot/chamfer drafts with measured entry
width and angle. Pocket roughing and outside roughing are distinct; outside clearing
protects the full workpiece envelope. Trace and smooth-loop roughing are retained
only for saved operations. See [2D milling](MILLING2D.md) for scope and limits.

Stock setup Apply and Save refit the blank when side, top or backing allowances
change; unchanged allowances preserve manual dimensions. Fitting margins persist
with the project and undo history. Incremental simulation strips are retained on
arrival so coalesced browser paints cannot discard stock-removal updates.

The 2.5D Roughing group includes **2D offset clearing**. Select a horizontal target
face or enter a numeric region/depth. Each stepdown recomputes raised-model
obstacles, generates nested offset contours, and links contours within connected
free regions. Holes below the cut plane do not become solid protected columns.
Manual exclusion rings and stock allowances remain supported. Effective stepover
is capped at 50% of cutter diameter for core coverage. Older linked
`area_rough` operations use the corrected calculation without changing saved IDs.
Toolpath resolution controls conservative grid residuals; calculation remains
bounded and is not an adaptive/constant-engagement algorithm.

Operation labels use machining terms consistently: Face milling, 2D pocket
clearing (rectangular/circular geometry), 2D contour (internal/external), Outside
clearing, Slot milling, Planar finishing, Bore finishing, Circular milling, Thread
milling, Contour engraving, Chamfer milling, Drilling, Peck drilling, 3D offset clearing
and Parallel finishing. Strategy IDs and user-supplied operation names remain
portable. Legacy trochoidal pocket/trace strategies retain their generators and
accurate labels, without being added back to the creation ribbon.

Slot retains its own through-slot illustration. Software solid rendering
preserves all imported workpiece triangles.


The [milling toolpath audit](TOOLPATH_AUDIT.md) records externally referenced
requirements and the supported contracts of all 20 current/legacy strategy IDs.
Linked 2D offset clearing now defaults to stock-boundary machining with edge
overrun; selected-face confinement remains an explicit option. Absolute-height
contours honor axial stepdowns unless stock is confirmed pre-roughed. Finishing
feed applies without spring passes. Circular wall passes have tangential leads
per depth; pocket entry honors ramp preset angle/feed. Below-stock edge finishing
requires an exposed-surface confirmation. Fixed-depth V grooves are labeled
Contour engraving, distinct from variable-width V-carving.

## Remaining stock and native arcs

Shared Rust generation tracks actual preceding cutter sweeps, validates automatic
wall/face/edge preparation and supports optional remaining-stock rapid clearance.
The browser exposes the clearance gap and G1-only fallback; native UI additions
remain paused. Linked outside clearing uses the complete model silhouette and
exterior-connected free regions; numeric geometry retains its rectangle. All four
implemented milling posts emit validated analytical G2/G3 circles/helices. Browser
simulation defaults to conservative whole-cell stock for rectangular blanks, with
mode-aware playback and OBJ export. Nonrectangular display stays sampled and
thread/undercut simulation remains unsupported. See
[remaining stock and arcs](REMAINING_STOCK_AND_ARCS.md).

## Geometry-driven operation families

The browser has one 2D contour command for internal/external walls. Rust resolves
its compensation side from the selected model boundary before linking the draft.
Pocket clearing groups rectangular, circular and model-region offset clearing in
one family; selected circular features resolve to circular clearing, while general
horizontal regions use model-aware offset clearing with islands. Numeric geometry
variants remain editable. Existing saved strategy identifiers and toolpath engines
are retained. Bore, circular and thread milling retain distinct cycles, and facing,
planar finishing and outside clearing retain their distinct machining purposes.

Software settings includes a manual Check for updates action, with current-version,
download, offline and failure status. A waiting update uses the existing workspace
autosave and activation flow through Save workspace and update.

## Vector pockets, rest machining and STEP finishing

Selected 2D pocket regions use direct inward offsets with round joins. The
planner subtracts islands and the model geometry above each cutting layer.
Model-linked 2D offset clearing also uses vector passes within its geometry
budget. Existing numeric pocket entry cycles retain their entry behavior.

Rest machining is available in Passes for selected pockets, selected Outside
clearing contours and 2D offset clearing. Add an earlier enabled roughing
operation with a larger cutter. The rest operation clips its offset passes to
where the current cutter can reach material that may remain, using the
conservative stock from preceding operations in the same setup. Uncertain cells
are kept in the rest region. Rest machining requires the vector planner.

Linked 2D contour and Chamfer milling preserve supported horizontal circular
edges from the original metric STEP when the imported mesh boundary confirms
placement and trim. This includes complete circles, partial arcs and verified
tangent line/arc chains. The usual stock, fixture and NC preflight still
applies. Unsupported placements, source units or joins follow the existing
compensated mesh chain. G2/G3 and supported G5 posts use the shared arc stream.
G1 posts sample it at the operation chord tolerance.

## Planar outside clearing and automatic calculation

Outside clearing can pick a closed horizontal exterior/island boundary, like 2D
contour. Its measured plane supplies the bottom; positive tip offsets leave stock.
Each layer protects the selected island and the projected model material above
the cutting plane. Selected contours use direct vector offsets with round joins
and retain only regions reachable from outside the stock. Dense projections
above the vector edge limit use the bounded grid planner. Between passes, the
tool uses stock- and fixture-aware clearance. A short move (up to 0.5 mm) that
reverses direction by more than 120° can use a checked lift and re-entry.
Open or internal pocket boundaries are rejected. Saved numeric and
whole-model outside jobs retain their previous interpretation; reselect geometry
to use planar clearing. Edge overlays remain visible over coincident solid faces,
and edge snap distances use normalized rays.

Software settings adds optional automatic generation after committed project edits,
with debounce, revision checks and busy/edit guards. Failures remain visible and do
not trigger a retry loop. Milling direction includes Optimized: climb on closed
loops, nearest compensated entry on open chains (either milling direction there).
Closed loop entry rotation retains winding. Redundant offset-pocket boundary laps
are omitted; finish and spring feeds remain separate. Native arc posting and all
clearance/fixture checks remain active.

Conservative stock preserves merged horizontal terraces and vertical shoulders for
flat/mixed cutter jobs. Curved-only jobs use a triangulated upper envelope with
shared vertex maxima and merged flat strips.
The conservative removal kernel and reported cell-volume bound remain unchanged;
the rendered envelope can retain additional material. This is a 2.5D approximation,
not solid CSG or an undercut simulation. Nonrectangular stock retains sampled columns.

## Operation colors and selection

Browser paths, list color strips and playback sections share stable strategy-family
colors. Rapid moves retain the operation color at reduced opacity. List
hover brightens the corresponding path without rebuilding its buffers. Playback
colors use Rust-provided move start/end times, including rapids, on a single
native playback slider. Pointer and keyboard scrubbing remain available; there
is no separate upper timeline bar. Thin separators distinguish adjacent operations
even when they share a type color. The categorical palette uses more separated
hues and retains saturation in dark mode.

Ctrl/Command-click toggles operations and Shift-click extends a contiguous range;
plain click selects one. The selected-path filter accepts the complete selection.
Context actions enable/disable the group in one validated Session transaction,
with one undo step, and can show/hide selected paths. Editing opens the active
operation. Selection is presentation state; operation order and NC are unchanged.
Stock simulation continues to process the complete chronological operation sequence
when path display is filtered.

## Camera, geometry picking and starter tooling corrections

Imported-vise configuration is docked in the Selection sidebar. Cursor orbit uses
visible-surface depth; the view no longer refits during orbit or stock playback.
Unavailable facing targets remain highlighted with their eligibility reason.
Blank-click selection clears pending inspection highlights. The File menu no longer
contains the tray demo command; test fixtures load through the ordinary file API.

New browser milling workspaces include eleven editable example tools; existing
libraries can add examples without replacing their assigned tools. New operation
drafts default to Optimized, while stored direction choices retain their meaning.
Open-floor picking keeps the Pocket strategy and selected boundary rather than
switching to AreaRough. Selected pocket regions use offset/plunge paths; numeric
pockets also support raster, ramp and helix settings. Rectangular offset pockets
enter the core and use orthogonal links between parallel offsets. Air-span trimming
and certified feed shortcuts retain complete tool/fixture clearance checks.

Flat and mixed-cutter stock jobs preserve sharp vertical shoulders instead of
interpolating ramps between unequal cell heights. Curved-only jobs retain the
continuous upper envelope. Removal and volume still use conservative 2.5D cells.

## Sequential milling and stock contact

See [MILLING_SEQUENCE_REVIEW.md](MILLING_SEQUENCE_REVIEW.md). Optimized milling now
uses proven local approaches and same-tool transitions; planar finishing can link
clear spans without repeated retracts. Playback includes configured change-station
travel and a configurable exchange-time estimate; actual NC still uses the
controller/operator handshake. Shared stock contact colors distinguish uncut,
rough-machined and finish-pass surfaces and reset on seeking. The existing Bore
cycle is labeled Bore finishing; circular pocket clearing explains helical entry.

## Generated-motion efficiency review

See [TOOLPATH_EFFICIENCY_REVIEW.md](TOOLPATH_EFFICIENCY_REVIEW.md) for before/after
measurements across all 20 strategies. Numeric exterior and 3D clearing share
stock-certified air trimming and compare long feed links with checked local
lift/rapid approaches. Prior whole-stock facing supplies subsequent roughing's
starting depth. Bidirectional facing may use the longer axis after fixture checks;
explicit one-way rasters retain their direction. Implicit Optimized clearance
includes hidden fixture geometry. Controller preflight and analytical arcs remain
shared with playback/simulation; these checks do not validate a native UI package.
Planar offset distance fields use physical axis/diagonal distances to avoid excess
laps from undersized straight-wall stepover; the 3D contact grid keeps its prior
conservative spacing. Dense coverage regressions include rotated regions/islands.
Planar clearing in every milling mode fits longer verified spans through angled
grid regions and blends their joins with tangent circular arcs. The generator
accounts for neighboring blends in the stepover budget and avoids reversing
inside a cut when a checked lift and re-entry is available. Playback and
linearized output use bounded samples of those curves.
Angled clearing automatically refines a coarse grid for narrow stepovers when
the finer grid fits the existing work limits. The WebGL viewport requests
antialiasing to reduce pixel stair steps on path lines.
Active machine setup and portable machine records select G1, G2/G3 or LinuxCNC
G5 output. Flat curves in G5 mode use bounded cubic spans, with circular output
for helices. Existing linear-only settings remain readable by the shared engine.

## Workspace issues

Inspect → Issues lists configuration findings for the current workspace and
preflight findings from the most recent generated path. Expand a row for the
details, a suggested correction, and links to the affected settings. Findings
with a move location also have a Look at action. It pauses playback at the
reported time, collapses the Issues panel and playback controls, and centers the
viewport on the highlighted contact. Back to issues reopens the findings with
the current move still highlighted.

The browser keeps calculated moves available for inspection and stock playback
when a machine travel, feed, fixture, or tool-change check reports a problem.
If the geometry cannot be calculated, Issues displays the settings or geometry
error. Approve marks a finding as reviewed until the workspace changes. Export
runs its own full preflight every time.

Program checks add typed entries through `src/issues.rs`. The workspace and
diagnostic playback commands use the same issue schema.
