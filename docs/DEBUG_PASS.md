# Debug pass — 0.5.1

Date: 2026-11-07. Scope: project/profile loading, file export, machining validation,
mesh import, inspector navigation and viewport controls. This pass adds no new
machining strategies.

## Reproduced and fixed

1. **Export could overwrite its input through a hard link.** Canonical filenames
   differ for two hard links to the same file. The old CLI returned success and
   replaced the input project's JSON with G-code. Import/export now compare file
   identities through the open-source `same-file` crate on Windows and Unix.
   The GUI's profile exporter also protects the open project. Comparison errors
   block output. Regression tests exercise real CLI imports and both export modes
   and verify that source bytes remain intact.
2. **Travel checking used different rounding from export.** `round(x * 10000)`
   rounds a tie differently from Rust's four-place decimal formatter. For example,
   X=0.03125 exports as X0.0312; the previous checker predicted X0.0313 and accepted
   a minimum of 0.03124. The checker now uses the exact export conversion and
   rejects that move. A regression test reproduced the old acceptance before the fix.
3. **Job errors blocked otherwise valid reusable profiles.** Saving/loading a
   machine/tool definition depended on the current job's operations. A pocket
   narrower than its tool prevented profile reuse. Profiles now validate their
   own setup and cutting data independently; applying one preserves job geometry
   and controller selection. Job regeneration still blocks incompatible toolpaths
   and G-code. Malformed or incompatible profiles remain rejected before mutation.
   Profile file errors now use the normal operation-status message, so a later
   successful save/load replaces them without masking machining validation errors.
4. **Inspector navigation scrolled away and sections shared scroll memory.**
   Stock/Machine/Tool navigation now sits outside the scrollable content. Inspector
   tabs and setup sections have separate scroll identities. A regression test uses
   the production widget spacing at 1080×720, scrolls Tool settings, checks that
   navigation remains visible, and confirms Machine opens at its own scroll offset.

## Verification

Reproduce the full checks with:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets -- --include-ignored
cargo test --locked --no-default-features
```

The ignored CAD test requires the bundled runtime; set
`OPENSOURCECAM_CAD_RUNTIME` to its `cad-runtime` directory when it is not next to
the executable. The full desktop test configuration contains 49 tests, including
five new regression tests. CLI-only tests exclude the six desktop UI tests.

Additional checks during this pass: 160 deterministic malformed/mutated ASCII STL
and OBJ inputs produced one valid import and 159 clean errors, with no panic,
abnormal process exit or five-second timeout. The native Windows machine/tool
layouts were rendered and inspected. Existing real-pointer orbit/pan tests,
process switching, old-schema migration, STEP/IGES units, post dialects and
clearance/retraction tests remain in the full suite.

The checks cover software behavior and generated data. They do not verify a
physical controller, machining result or fixture/holder collision simulation.
