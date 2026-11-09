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
| Browser presentation | web/app.js, index.html, style.css | Touch-oriented controls, workspace persistence, file downloads |

Both `--command` in the CLI and the browser dispatch through `commands::execute`.
Command ids are generate, export, simulate, plan and surface. For example:

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
Python/OCP runtime; browser STL/OBJ parsers use bounded in-memory input.

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
History is bounded to 30 entries / 8 MiB of serialized snapshots and resets on reload.
Large snapshots can exhaust the history budget; autosave and downloaded backups
remain separate protections. A typed draft may temporarily contain invalid machining
values so it can be repaired; generation/export still use the full existing validation.
The browser keeps only a presentation copy of the committed session view. Its worker
owns the session and calculates against that state; JavaScript has no undo stack.
