

⚠️⚠️⚠️ Author's note:
Hey, Author here, before the AI slop. This program was made with AI (Codex) so I don't promise anything.
It's "vibe coded trash", but I needed a CAM software that I could use for my DIY machines preferably free. Something that works with all. This is my attempt at trying to make an open source software for every cnc enthusiast from printer to wire edm to milling. 
Contributions are welcome and feel free to post issues. I can't guarantee I'll keep updating regularly since I have plenty to do, but I'm trying to test everything with what I have.

Thank you for checking out my project,
Author 



# OpenSourceCAM

An open-source CAM workbench for milling experiments, written in Rust.
The browser workbench (PWA) is the primary development target for phones, tablets
and PCs. It can also be hosted on a shop server. Native Windows/Linux development
and automatic native release packaging are paused; their source remains available.

**Development prototype · MIT licensed · Not validated for production machining**

[Browser preview](https://noob-p2w.github.io/OpenSourceCAM/) · [Browser & shop hosting](docs/PWA.md) · [Legacy native source & build](docs/NATIVE_GUIDE.md) · [Known scope](docs/ROADMAP.md)

The browser preview is served through GitHub Pages when enabled. It contains the
current browser interface, not every feature of the native application.

[Earlier native workbench screenshot](docs/workbench-workflow.png) (native development paused).

## What exists today

| Area | Current implementation |
| --- | --- |
| Milling | Rectangular pockets and profiles, peck drilling, supported model-linked stock machining, 3-axis offset roughing and parallel finishing |
| Inspection | Toolpath preview, approximate machining time, sampled material-removal simulation |
| Output | Metric G-code with Generic, GRBL, Mach3 and LinuxCNC posts, subject to each post's supported configuration |
| Projects | Local versioned JSON project files with embedded geometry |
| Native app (paused) | Existing source and build instructions retained; no routine native package builds |
| Browser app | Desktop/tablet ribbon and operation tree, phone layout, dedicated setup/operation windows, machine/tool tables, oriented rectangular/round stock, example vise, STEP/IGES/STL/OBJ import, playback, sampled simulation and G-code download |

The browser runs the shared Rust CAM engine locally through WebAssembly. There
is no project-upload service, account system or machine-control connection.
A web host serves application files; projects stay on the device unless the user
exports or shares them. Browser storage can be cleared: download project backups.

## Try the browser workbench

Open the browser preview, choose the tray demo, and select **Generate** to inspect
its paths. Project files and G-code can be downloaded from the workbench.
Installation and offline caching require HTTPS and browser support. Actual
phone/tablet hardware compatibility has not been exhaustively tested.

To self-host during development, use the shop ZIP from a successful
[workflow build](https://github.com/Noob-P2W/OpenSourceCAM/actions/workflows/web.yml), extract it and follow its
`README.txt`. The Windows launcher opens a browser and prints the shop-network
address. This development launcher requires Python 3.10+; it is not a standalone
installer. See [hosting instructions](docs/PWA.md) for HTTPS and build details.

## Limits you should know

- This is experimental CAM software. No claim is made about machining safety,
  dimensional accuracy, reliability, or suitability for a particular machine.
- Simulation is sampled. It is not complete machine collision verification.
  Review G-code, tool data, workholding, offsets and controller behavior separately.
- Browser STEP/IGES import uses a bundled OpenCascade WebAssembly kernel.
  Imported CAD is tessellated for the existing mesh-based CAM workflow; assemblies
  are flattened. Detailed native editors and solid rendering are not fully ported.
- Milling is the implemented machining process. Other process workspaces do not
  imply implemented toolpaths or export engines.
- No direct CNC streaming/control is included. The controller's computer executes
  the exported program.
- The roadmap records ideas and scope, not delivery commitments or dates.

## Develop and verify

Start with the browser workflow. Legacy native build instructions remain in the
[native guide](docs/NATIVE_GUIDE.md). For the browser:

```sh
npm ci --prefix web
bash scripts/build-web.sh
python3 scripts/package-web.py
python3 -m http.server 8080 --directory dist/pwa
```

Browser tests and packaging instructions are in [docs/PWA.md](docs/PWA.md).
The repository includes Rust regression tests and a Chromium smoke test for the
browser workflow; passing them does not establish production readiness.

## License and contributions

Project code is available under the [MIT license](LICENSE), without warranty.
Bundled dependencies retain their own licenses; see
[third-party notices](THIRD_PARTY_NOTICES.md). Bug reports with reproducible
projects and clear steps are welcome. Avoid including personal data, proprietary
parts, or machine credentials in public issues or attachments.

## Development workflow

See [architecture](docs/ARCHITECTURE.md) and the
[native/browser feature inventory](docs/FEATURE_PARITY.md). After prerequisite
setup, `python3 scripts/check.py web` runs the browser quality gate, including
Rust engine tests, WASM packaging and browser checks. CI also runs headless Rust
formatting, Clippy and tests with `--no-default-features`.

Native packaging runs only when explicitly requested through the **Shared engine
checks and optional native build** workflow with **build_native** enabled.
`python3 scripts/check.py native` and `all` remain available for native work;
packaged native CAD tests are not part of routine browser validation.
