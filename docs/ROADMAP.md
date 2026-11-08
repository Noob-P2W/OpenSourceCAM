# Development status

Version 0.9 prioritizes actual machining: supported imported parts can generate
facing, cavity milling, circular interpolation and exterior stock clearing, with
model links, sampled stock playback and imported fixture contact checks.
See [Machining from stock](MACHINING_FROM_STOCK.md).

Work on one milestone at a time. Keep the Rust app lightweight, support Windows
and Linux, use open-source project dependencies, and bundle components needed by
the release rather than requiring users to install another CAD application.

1. **File import — implemented in 0.2.** Native ASCII/binary STL and polygon OBJ;
   bundled OpenCascade for STEP/STP and IGES/IGS; explicit mesh units, placement,
   stock fitting, shaded model preview and portable JSON persistence. Imported
   models are reference geometry; they do not yet drive automatic 3D machining.
   Original CAD exchange text is retained for future feature work. Future formats:
   DXF/SVG for 2D profiles and additional mesh/CAD formats as needed.
2. **Process selection — interface implemented in 0.3.** Main-menu cards for
   milling, turning, 3D printing, laser cutting, plasma cutting, grinding, wire
   EDM, electrode/sinker EDM and Mixed. The selected type persists in the project.
   Top buttons create milling operations; the left list selects/enables them;
   Operation/Setup/Model inspector tabs separate settings. Other process engines
   are explicitly planned: reference models can be imported and saved, while
   unsupported operations/export are disabled and blocked in the core too.
3. **Postprocessors — milling implemented in 0.4.** Generic, Mach3 Mill, GRBL 1.1
   and LinuxCNC profiles with saved G54–G59 offset and flood coolant settings,
   controller-specific modes/endings, CLI overrides and regression tests. Klipper
   is a planned 3D printing profile; export is blocked until slicing/extrusion is
   implemented. Other processes gain posts alongside their motion engines.
4. **Machine setup and tooling — initial implementation in 0.5.** Stock/Machine/
   Tool inspector sections, reusable JSON machine/tool profiles, XYZ travel and
   work-origin definitions, feed/spindle maxima and process-specific tool fields
   for all eight standard families. Milling checks generated tool-tip travel and
   optional flute/stickout reach; configured rapid feed drives time estimates.
   Non-milling definitions persist as reference setups until their motion engines
   and kinematics are implemented. Version 0.6 adds the dedicated local tool library, per-operation tool/preset
   assignments and Mach3/LinuxCNC milling ATC. Turning turret and exchangeable
   mixed-head metadata are stored; their motion engines remain future work.
   The user requested this library before continuing the remaining milestones.
   See [tool library](TOOL_LIBRARY.md).
5. **Feature recognition — initial implementation in 0.7.** Rust mesh analysis
   finds top-accessible vertical circular holes and axis-aligned rectangular
   blind pockets. Candidate review highlights boundaries/depth and creates disabled
   operations with explicit tool/preset bindings. Closed/oriented topology, complete
   wall profiles, tool suitability and machine limits are checked. Unsupported
   geometry is skipped; arbitrary contours, deeper-level features and analytic
   CAD feature recognition remain future extensions. See
   [feature recognition](FEATURE_RECOGNITION.md).
6. **Machine workspace — reference-model stage in 0.8.** Independently import
   machine, vise and fixture models; set source units, XYZ placement/rotation and
   work/machine coordinate frame; save embedded geometry and visibility in projects.
   Detailed machine and operation editors are included. Machine articulation,
   broader solid collision verification remain future work. Version 0.9 adds conservative
   cutter/shaft/holder checks against imported fixtures and sampled stock removal.
   See [setup and workspace](SETUP_AND_WORKSPACE.md).
7. **Viewport and bug fixes.** Orbit rotation was included in milestone 1 because
   imported 3D parts need inspection. Left drag orbits; right/middle/Shift drag
   pans; wheel zooms. Further usability fixes remain ongoing.
8. **Mixed — developed last.** DIY and special machines have a menu/workspace
   entry now. Custom process chains, machine setup, tooling and motion generation
   will be implemented after the standard process families.

Further work: arbitrary 2D contours, robust analytic CAD recognition, ramps/helixes,
adaptive clearing, general 3D finishing, second setups, articulated kinematics,
and the remaining process motion engines. Do not imply physical production verification.
