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
| Stock simulation | Numeric and rendered | Numeric report |
| Geometry display | Native solid/mesh renderer | Wireframe |
| Machine/tool setup | Detailed native editors | CNC profile selection, limits/origin, tool/preset selection; advanced settings still use JSON |
| Stock fitting and workholding | Supported | Setup step: fit stock, part offsets, vise/fixture transforms |
| Feature recognition | Supported | Operations: candidate review, unsupported/open-mesh diagnostics and disabled feature operations |
| Editing / undo history | Native implementation | Rust Session; native desktop history migration pending |
| Autosave / recovery | Native save protections | Local current/previous autosave and draft recovery |
| Install/offline updates | Native packages | Service worker, version display and update prompt |
| Controller streaming | Not implemented | Not implemented |

Verification currently includes Rust regression suites and Chromium phone-sized
and tablet-sized browser checks. These do not establish physical iOS/Android
coverage, complete collision verification, or machining accuracy. Record real
hardware reports with browser/OS versions and the visible PWA build id.
