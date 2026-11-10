# Native / browser feature coverage

This is an implementation inventory, not a release promise or certification.
“Shared” means the same underlying Rust logic; device verification is separate.

| Capability | Native | Browser |
| --- | --- | --- |
| Project JSON and migrations | Supported | Shared loader |
| Bounded STL/OBJ import | Supported | Shared parsers; new fitted project |
| STEP/IGES import | Bundled runtime | Bundled OCCT WASM; tessellated mesh, source retained |
| Supported milling toolpaths and export preflight | Supported | Shared Rust core |
| Generate/export/simulate/plan command boundary | CLI --command | WASM worker |
| Stock simulation | Numeric and rendered | Shared sampled mesh and numeric report, detail selection, live bounded playback snapshots on the same estimated-time timeline as the library tool assembly, and OBJ download |
| Geometry display | Native solid/mesh renderer | Depth-tested WebGL solids, stock volume, feature outlines, orbit/pan/zoom, axonometric default, top/front/right/isometric, grid/axes controls and fit; sampled software fallback |
| Machine/tool setup | Detailed native editors | Searchable machine/tool tables, persistent portable machine+post records, Generic/GRBL/Mach3/LinuxCNC templates, CNC profile import/export, configuration/tool changer, tool geometry/presets and assembly settings |
| Process opening menu | Nine process workspaces | Same Rust catalog and saved process identity; planned engines remain disabled |
| Tool library | 37 kinds and assembly preview | Same catalog, assembly geometry and focused dimension arrows (mm/in); local creation, search, editing, duplication, guarded removal, preset duplication/removal, inch display/editing, merge/replace import and JSON export |
| Planning controls | Blank allowances and operation replacement | Same PlanOptions in the worker; notes, XY/top/backing, 2.5D choices and explicit replacement |
| Detailed operation editor | All implemented settings | Dedicated operation drafts with geometry/tool/pass/feed/height tabs, atomic validation/save, geometry unlinking and keyboard/context actions |
| NC inspection | Preview and export | Checked code preview and download |
| Stock fitting and workholding | Supported | Modal Setup: rectangular/round physical stock, XYZ stock/part orientation and offsets, shape-preserving fit, configurable example vise/backing and imported fixture transforms |
| Feature recognition | Supported | Operations: candidate review, unsupported/open-mesh diagnostics and disabled feature operations |
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
collision verification. Playback uses a fixed preview duration and the shared Rust estimated-time timeline.
With Simulated stock enabled, bounded 32-cell worker snapshots update during
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
