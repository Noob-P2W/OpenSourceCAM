# Sequencing and machining-contact review

Optimized operations use a 0.5 mm remaining-stock clearance by default (limited by
operation clearance). An explicit Remaining-stock clearance overrides that gap.
Climb/conventional modes with an empty clearance field retain full-clearance
links. All low routes check whole-cell upper material bounds, the cutting tool,
shaft, defined holder, and fixture tree. Unknown/exhausted proofs retain full
clearance. No target-model void or externally pre-cleared assertion certifies air.

Consecutive enabled operations using the same tool can carry the proven stock
state and local clearance across their boundary. A real tool change and program
end still retract to setup travel clearance. Selected planar outside-clearing
layers choose a nearby entry on their first loop without reversing its winding.
Planar finishing joins adjacent raster spans at cutting feed only when the full
route is proven clear. One-way rasters retain a lifted return to avoid dragging backwards across the
finished face. Otherwise it retracts; a local approach rapids only to
remaining material plus clearance and feeds the final approach. There is no rapid
at a floor-contact height. Coverage tests check the finished floor independently.

Coplanar path lines now receive depth separation from solids so horizontal finish
passes remain visible; raised geometry still occludes paths behind it.

## Tool changes

The NC post retains M5/M9, required manual M0 or the configured controller M6
handshake, calibrated G43 H registers, restored units/distance/arc/feed/work-offset modes with G40/G80, checked return
positioning, and travel/fixture preflight. ATC with an explicit change station
prepositions at that station before M6, without lowering back to clearance first.
The configured macro must return to that station; its internal magazine/spindle
kinematics remain controller-owned.
The first automatic M6 still relies on the controller's safe startup routine; the
CAM program cannot infer the machine's unknown initial position.

Playback adds checked change-station approach and return moves, and an exchange
pause using Tool-change time estimate (default 30 seconds). This is an estimate;
a manual M0 waits for the operator, and an M6's actual time/kinematics belong to
the controller. The tool is hidden during exchange and changes before the return.
Without a configured station, playback explicitly identifies the unknown
controller/operator route and does not invent magazine-arm motion. Manual NC
export continues to require its station; GRBL multi-tool export remains blocked.
Changes and non-cutting waits do not remove stock. Inspection, playback, rewind,
and stock export use the same expanded time sequence.

## Stock colors and operation names

Uncut stock is blue-grey, rough-machined stock amber, and finish-pass contact green.
The shared stock engine records contact even when a finishing pass does not lower
an already roughed floor. Planar finishing does not relabel raised vertical walls
as finished. Wall finishing records its cutter-height range. Contact colors are
sampling-resolution provenance, not a measured surface finish, tolerance or
complete-machining certification. Seeking and edits rebuild this state.

Bore finishing names the existing continuous helical wall-finishing cycle. Circular
pocket / hole clearing removes solid material; its Helix entry opens a hole with
an end mill. These remain distinct operations because their stock preparation and
cutting engagement differ. Saved strategy IDs are unchanged.

## Sources reviewed

- [Autodesk: reducing air cutting](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/REDUCE-AIR-CUTTING-POCKET.htm)
  and [2D pocket](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-POCKET-READ.htm): remaining material and linking are distinct from target geometry.
- [Haas M06](https://www.haascnc.com/service/codes-settings.type=mcode.machine=mill.value=M06.html)
  and [LinuxCNC M6](https://linuxcnc.org/docs/stable/html/gcode/m-code.html#mcode:m6): tool-change behavior is controller-specific; length compensation and safe positioning require explicit configuration.
- [Fusion community: How to force tool to stay down?](https://forums.autodesk.com/t5/fusion-manufacture-forum/how-to-force-tool-to-stay-down/td-p/14049374)
  and [Milling 2D Pocket won't stay down](https://forums.autodesk.com/t5/fusion-manufacture-forum/milling-2d-pocket-won-t-stay-down/td-p/13863825): practical reports of unnecessary lifts motivate checking connected regions, linking and real stock rather than simply removing retracts. Forum settings are not universal machine-safety rules.

The algorithms and regression fixtures are original; no vendor/forum code was copied.
