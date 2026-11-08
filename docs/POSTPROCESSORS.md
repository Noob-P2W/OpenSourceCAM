# Controller postprocessors

Version 0.6 provides four metric, absolute milling posts and configured ATC
for Mach3/LinuxCNC. Select
one in the ribbon's **Post** command or the inspector's **Post** tab. The selection,
G54–G59 work offset and coolant setting persist in JSON projects (schema 6 in version 0.6).

| Profile | Path mode | Delimiters | End |
| --- | --- | --- | --- |
| Generic metric | Controller default | None | M2 |
| Mach3 Mill | G61 exact stop | `%` | M30 |
| GRBL 1.1 | Controller planner settings | None | M30 |
| LinuxCNC | G61 exact path | `%` | M30 |
| Klipper | Planned 3D printing engine | Export unavailable | — |

All milling posts emit G21, G90, G17, G94, G40, G49 and G80, then the selected
work offset and M9. Z retracts to clearance before XY positioning. The spindle
uses M3/M4 and the assigned preset RPM. Flood coolant adds M8 after spindle start.
Use tool-preset coolant when enabled; otherwise the job-level Off/Flood choice
overrides all presets, including an explicit Off. Each program ends at clearance with M5 and M9.

Moves are explicit G0/G1 coordinates with four decimal places; drilling pecks
are expanded into feed/retract moves. Controller-side radius compensation,
homing and machine-coordinate return moves are absent. In mounted-tool mode,
use one tool and establish the selected work offset at the stock's lower-left
corner/top using the actual tool tip. Clearance
is relative to that stock surface. Controller settings and installed machine
hardware determine spindle, coolant, speed and acceleration behavior.

GRBL output omits percent delimiters, G61, canned drilling and tool-change
commands. Text comments are ASCII, sanitized and bounded; regression tests keep
generated lines below 80 characters. GRBL's planner/junction deviation settings
govern cornering. Mach3 exact-stop mode and LinuxCNC exact-path mode can increase
actual cycle time compared with the constant-feed estimate.

The original native vector ribbon icons and post implementations are MIT-licensed
OpenSourceCAM code. No Autodesk code or artwork is included.

## Milling ATC

Choose Milling ATC in Setup → Machine, enter capacity and enable controller-managed
safe M6 handling only after configuring that routine on the controller. Each
used tool needs an assigned pocket within capacity and its own calibrated H
length-offset register. Tool numbers and used pockets must be distinct. Set
manual-change off. The NC uses tool numbers; pockets describe the controller's
configured tool-to-pocket mapping and are not a guessed carousel movement.

The initial sequence stops spindle/coolant, emits `Tn M6`, then `G43 Hn` before
moving to work clearance, restoring the planned XY position and starting the
selected spindle direction/coolant.
Between tools it first retracts the current tool to work clearance, stops
spindle/coolant, changes the tool, applies the next H offset and reestablishes
clearance and XY position. Explicit XY restoration also covers consecutive tools
working at the same hole, where the planner omits a zero-distance move. The controller M6 routine must own safe machine positioning for the
change; the application does not invent a safe changer coordinate or write G10
calibration values. The program begins with G49, so initial safe positioning
inside M6 must not depend on an assumed active tool-length offset.

Multi-tool mounted-mode output, ATC on Generic/GRBL, missing/conflicting
calibration registers, manual-change tools, unsupported break control and
nonzero head mounting offsets are rejected before export. D offsets are stored
but G40 remains active because radius compensation is baked into the path.
Same-tool preset changes update RPM/direction/coolant without another M6.

Turning turret/station and offset data are stored and checked, but no turning
program is emitted yet. Mixed head changes and Klipper extrusion output remain
planned. Tool-change time is excluded from the estimated cycle duration.

Controller behavior references: [LinuxCNC tool compensation](https://linuxcnc.org/docs/html/gcode/tool-compensation.html)
and [Mach3 Mill manual](https://www.machsupport.com/wp-content/uploads/2013/02/Mach3Mill_1.84.pdf).
Physical ATC/controller acceptance has not been tested on a machine.

## Command line

```sh
opensourcecam --posts
opensourcecam --export project.cam.json selected-post.nc
opensourcecam --export-post mach3 project.cam.json mach3.nc
opensourcecam --export-post grbl project.cam.json grbl.nc
opensourcecam --export-post linuxcnc project.cam.json linuxcnc.nc
```

The override preserves the project's offset/coolant and leaves its file untouched.
Unsupported process/profile combinations fail before writing output. Klipper is
reserved for the printing workspace; milling motion is never relabeled as
extrusion output. Physical controller/machine acceptance has not been verified.

## Primary controller references

- [Mach3 Mill installation/configuration guide](https://www.machsupport.com/wp-content/uploads/2013/02/Mach3Mill_Install_Config.pdf): exact-stop and constant-velocity behavior.
- [LinuxCNC G-code reference](https://linuxcnc.org/docs/stable/html/gcode/g-code.html): G61 exact-path semantics and work offsets.
- [GRBL supported commands](https://github.com/gnea/grbl/blob/master/doc/markdown/commands.md): supported modal groups and controller configuration.
- [Klipper G-code reference](https://www.klipper3d.org/G-Codes.html): printing/extrusion commands for the future printing engine.
