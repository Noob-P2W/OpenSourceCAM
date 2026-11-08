# Tool library validation — 0.6.0

Date: 2026-11-07. Scope: local library workflow, tool/head schemas, project
assignments, machine-profile compatibility, per-operation planning and milling ATC.

## Bugs fixed while iterating

- Tool editor columns could collapse and push Save/Cancel outside the window.
  The editor now reserves its parameter and preview regions, with a fixed footer.
  UI tests open all six tabs for every one of the 37 types at 1080×720 and
  1440×940 and check footer/preview bounds.
- New operations could forget the selected cutting preset. Both tool and preset
  references are now saved and tested through the real library controls.
- Import could overwrite tool identities or inherit conflicting device bindings.
  Default import merges atomically, remaps holder links and assigns fresh numbers;
  machine pocket/offset/turret assignments are cleared. Explicit replacement is
  refused when job operations exist. Regression tests check both paths.
- A job-level coolant Off could be obscured by preset Flood. The explicit preset
  coolant switch now determines which source is used, and Off override is tested.
- Different ATC tools could share an H register, or move after M6 without
  restoring XY when consecutive tools used the same location. Separate calibrated
  H registers are required; initial and subsequent changes explicitly restore the
  validated XY position at clearance. A same-hole two-tool regression covers the
  planner's intentionally omitted zero-length positioning segment.
- Machine profiles could carry stale global tool settings after library adoption.
  New profiles are machine-only; legacy profiles add a definition without replacing
  other library tools. Application remains atomic and tested.
- Printing validation now checks both first-layer and normal volumetric flow,
  heater ratings and layer geometry. Cutting-head current/pressure and spindle
  rated-speed limits are also enforced. Missing/unknown/nonfinite fields fail.
- Scroll offsets are independent per editor tab and tool. Preview mesh triangles
  avoid the feather artifacts produced by polygon antialiasing on revolved faces.

## Checks

The full desktop configuration contains 67 automated tests. Commands:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets -- --include-ignored
cargo test --locked --no-default-features
```

The ignored CAD test requires the bundled importer. Windows native screenshot
captures check the list, type picker, geometry editor and printing data editor.
Linux checks run in WSL, including CPU UI tests and bundled CAD import; the Linux
desktop has not been visually exercised in a real graphical session.

Package checks compare source/executable content and inspect a STEP file using
the packaged runtime. The sample catalogue must validate with `--inspect-library`;
the library and ATC example jobs must validate; ATC NC output must agree across
Windows and Linux. No physical machine, heater, tool changer or controller has
been connected in this pass. Stored definitions for planned motion engines do
not establish usable machining output for those processes.
