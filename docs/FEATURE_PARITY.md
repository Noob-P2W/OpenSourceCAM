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
| Stock simulation | Numeric and rendered | Shared sampled mesh and numeric report, detail selection, playback progress and OBJ download |
| Geometry display | Native solid/mesh renderer | Depth-tested WebGL solids, stock volume, feature outlines, orbit/pan/zoom, top/front/isometric and fit; sampled software fallback |
| Machine/tool setup | Detailed native editors | CNC profile import/export, configuration/tool changer, tool geometry and presets, shaft/holder/post settings |
| Process opening menu | Nine process workspaces | Same Rust catalog and saved process identity; planned engines remain disabled |
| Tool library | 37 kinds and assembly preview | Same catalog and assembly geometry; local creation, search, editing, duplication, guarded removal, preset duplication/removal, inch display/editing, merge/replace import and JSON export |
| Planning controls | Blank allowances and operation replacement | Same PlanOptions in the worker; notes, XY/top/backing, 2.5D choices and explicit replacement |
| Detailed operation editor | All implemented settings | All settings, inherited overrides, geometry unlinking, strategy picker and keyboard operation actions |
| NC inspection | Preview and export | Checked code preview and download |
| Stock fitting and workholding | Supported | Setup step: fit stock, part offsets, vise/fixture transforms |
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
inspector is resizable in the browser at wide screen sizes; tablet/phone layout
remains responsive.

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
