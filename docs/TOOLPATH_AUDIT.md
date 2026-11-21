# Milling toolpath audit

Reviewed 17 November 2026. This covers all 20 implemented strategy IDs, including
legacy strategies that remain readable but are absent from the creation ribbon.
The checks concern the supported geometric methods, not a claim of feature parity
with Fusion or certification of every possible machine, cutter and workholding setup.

## External references and requirements

Public manufacturer and CAM documentation was read to establish requirements.
The changes and geometric tests are independently implemented; reference programs
and proprietary algorithms were not copied.

| Reference | Requirement applied here |
| --- | --- |
| [Autodesk: 2D Offset Roughing](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-POCKET-READ.htm) and [open/closed machining boundaries](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/GUIDE54328F8-4F1E-4838-8E3F-76D81F7B524F.htm) | Distinguish a confined face region from stock-boundary clearing. A confined boundary must not be presented as clearing surrounding stock. |
| [Autodesk: 2D Contour](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/GUID75B6821B-DE26-4E3B-AF10-4A54131CD9E4.htm) | A selected lower edge supplies the final reference height; axial passes and finishing passes have distinct purposes. |
| [Autodesk: Circular](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-CIRCULAR-READ.htm) and [Bore](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-BORE-READ.htm) | Discrete circular wall passes need leads at each level; continuous helical wall cutting is a different method. |
| [Autodesk: Chamfer](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-CHAMFER-READ.htm), [Slot](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-SLOT-READ.htm), [Trace](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-TRACE-READ.htm), [Engrave](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-ENGRAVE-READ.htm) | Separate edge chamfering, constant-width slotting, curve following and variable-width V-carving. Our fixed-depth V-grooves are labeled Contour engraving, not represented as Fusion-style V-carving. |
| [Harvey Performance: tool entry](https://www.harveyperformance.com/in-the-loupe/types-tool-entry/) | Entry method, angle and feed depend on the tool/application. Honor preset ramp angle/feed; maintain center-cutting restrictions and limit entry geometry to the selected region. The existing conservative 3-degree ceiling remains; a lower preset angle takes precedence. |
| [Harvey Performance: climb/conventional milling](https://www.harveyperformance.com/in-the-loupe/conventional-vs-climb-milling/) | Feed direction depends on spindle rotation and the material side, not simply CW versus CCW. Use one shared rule for the settings display and generator. |
| [Harvey Performance: trochoidal milling](https://www.harveyperformance.com/in-the-loupe/introduction-trochoidal-milling/) and [Autodesk: Adaptive Roughing](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/2D-ADAPTIVE-READ.htm) | Loop-shaped motion alone does not establish controlled engagement. Do not advertise the offset or legacy trochoidal algorithms as constant-engagement adaptive clearing. |
| [Haas: G81](https://www.haascnc.com/service/codes-settings.type=gcode.machine=mill.value=G81.html), [G83](https://www.haascnc.com/service/codes-settings.type=gcode.machine=mill.value=G83.html), [circular/helical interpolation](https://www.haascnc.com/service/codes-settings.type=gcode.machine=mill.value=G03.html) | Distinguish continuous feed drilling from full-retract peck drilling; keep XY rapids at clearance and re-entry in verified air. Helical pitch must agree with axial travel per revolution. |

Manufacturer material-specific feeds, engagement percentages and cutting speeds
were not substituted for user tool/material presets. Individual controller-specific
canned-cycle behavior was not assumed to be universal across postprocessors.

## Corrections made

- Linked 2D offset clearing defaults to **Stock boundary**, including cutter overrun
  at open stock edges. **Selected face boundary** retains confined machining. The
  selected face still supplies the bottom height. Manual numeric regions retain
  their selected-boundary default; either boundary mode can be chosen explicitly.
- Buried flat contour chains now apply axial stepdowns from proven cleared stock
  height to the absolute target edge height. A pre-roughed contour can explicitly
  use a single finishing level. Positive offsets and axial allowances still keep
  the final tool tip above the selected floor.
- Finish-feed overrides apply to final finishing passes without requiring spring
  passes. Cycles without such a pass reject unsupported finish-feed settings.
- Circular wall milling approaches and leaves the wall on tangent quarter-circle
  leads at every depth; Z changes occur in the pre-cleared region, off the wall.
  Preset lead feeds are used, capped at the applicable cutting/finishing feed.
- Pocket ramp and helix entries honor lower tool-preset ramp angles and ramp feed.
  Helix radius scales with cutter diameter and available room instead of a fixed
  1 mm cap; the overlap keeps the center covered rather than leaving an uncut core.
- The display and generator share climb/conventional resolution. The rule includes
  the free-region outer boundaries and reverse-wound islands of 2D/3D clearing,
  plus legacy trochoidal pocket boundaries and M3/M4 spindle rotation.
- Buried chamfer, engraving and trace references require an exposed-surface
  confirmation unless earlier facing proves the reference is exposed. Geometry
  below stock top is not, by itself, evidence that the stock has been removed.
- Linked outside clearing follows the complete model silhouette, preserving enclosed
  cavities; numeric clearing retains its rectangle. Fixed-depth engraving is labeled
  **Contour engraving** and describes its distinction from variable-width V-carving.

## Strategy-by-strategy verification

| Strategy | Verified contract | Supported-method limits |
| --- | --- | --- |
| Face milling | Full blank coverage, edge overrun, depth layers, one-way/bidirectional raster, final feed and spring passes | Stops at model top; does not clear recessed faces through raised features |
| Rectangular pocket clearing | Raster/offset coverage of the cutter-accessible region, all four wall passes, ramp/helix/plunge entry, allowances | Numeric/recognized supported rectangles; inside corner radius remains tool-limited; use model clearing for islands |
| Circular pocket clearing | Concentric clearing reaches floor and wall, center coverage, radial compensation, entry modes | Clears solid stock; distinct from a pre-cleared bore wall-finishing cycle |
| 2D offset clearing | Stock sides and corners, selected-region confinement, nested loops, connected links, model protection at each depth, core coverage | Bounded grid can leave conservative residual stock; no constant-engagement solver |
| Outside clearing | Covers exterior stock and diagonal corners; retains model silhouette and enclosed cavities | Linked models use their silhouette and exterior-connected regions; numeric geometry retains a rectangle |
| Internal 2D contour | Tool-radius/allowance compensation, absolute target height, axial layers, spindle-relative direction and final feed | Flat chains or numeric rectangles; not bulk island/pocket removal |
| External 2D contour | External compensation, rounded numeric corners, axial layers, positive floor offset and final feed | Selected contour/profile only; not an automatic area-clearing operation |
| Slot milling | Constant-width centerline, matching cutter, depth layers, finishing repeat and feed | Requires matching cutter width; wider slots must be cleared separately |
| Planar finishing | Horizontal region, island clearance, target floor offset, full reachable face coverage, final feed | Preceding roughing proof or explicit externally prepared stock; not a roughing or 3D-surface strategy |
| Bore finishing | Cylindrical/conical internal helix, external cylinder, bottom cleanup circle, pitch/stepdown and finish feed | Pre-cleared wall cycle; external taper is rejected; radial entry/exit rather than helical lead ramps |
| Circular milling | Separate circular depth passes, tangent leads in cleared air, side compensation, finish and spring passes | Pre-cleared cylindrical hole/boss; tapered walls direct the user to Bore; contour passes do not remove an entire solid pocket |
| Thread milling | Measured cutter pitch/tooth/neck, radial thread depth, helix pitch, hand, clearance and fixed-depth exit | Pre-cleared cylindrical, single-lead cycle; no conical/multi-lead threads or helical lead ramps |
| Contour engraving | Measured V-cutter cone, selected path, penetration, varying reference heights and final feed | Fixed-depth contour grooves; no medial-axis variable-width text filling |
| Chamfer milling | Edge compensation from cutter angle/width/penetration, target height, measured angle match and final feed | Flat chains; no 3D edge chamfer or independent collision-avoidance tool-axis tilt |
| Drilling | Single feed to drill-tip depth, R plane, XY at clearance | Drill-tip depth is not automatically a full-diameter breakthrough depth |
| Peck drilling | Incremental pecks, full R-plane retract, re-entry clearance in the previously drilled bore | No chip-break-only, dwell, variable-peck or controller-specific cycle variants |
| 3D offset clearing | Conservatively protected model/contact field, layered connected offset regions, stock-envelope coverage, direction | Top-accessible closed meshes, flat/ball tools and bounded grid; no undercut machining |
| Parallel finishing | Flat/ball contact heights, adaptive span sampling, stock fit and prior-roughing checks | Top-accessible surfaces; no 5-axis or undercut finishing |
| Trochoidal pocket milling (legacy) | Translating loops, full reachable rectangular coverage, boundary cleanup, limited advance and direction | Not constant engagement; not restored to the creation ribbon |
| Trace milling (legacy) | Varying-Z centerline, optional supported planar compensation, exposure guard and feed | Varying-Z compensated offsets remain unsupported; not restored to the creation ribbon |

## Evidence and remaining improvements

`tests/toolpath_audit.rs` samples cutter sweeps independently of the generators,
including material at stock sides/corners, wide requested stepovers, and both
pocket patterns. It checks absolute depth layers, finish-feed effects with zero
spring passes, helical pitch, circular off-wall approaches, preset ramp limits,
unsupported options, spindle direction and exposed-surface guards.

Existing suites additionally exercise all 20 strategies, implemented posts,
fixture/tool-envelope rejection, physical-stock profiles, contact tolerances,
model-link invalidation, setup transforms and simulation. Browser tests cover
boundary-mode persistence and all-four-side overrun after picking a face.

Pre-cleared/exposed-surface confirmations remain operator inputs. Automatic
preparation checks now also use conservative preceding cutter-sweep certificates.
Custom 2D selections still require accessibility
review; this audit does not establish a universal target-solid gouge proof.

This is a geometric and behavior audit of implemented methods. It does not add
exact evolving-solid rest machining, load-adaptive engagement, variable-width
V-carving, machine deflection prediction, or automatic tool/material-specific
cutting-data selection. Those remain explicit feature work. Analytical circles/helices
now emit G2/G3
with conservative preflight and a G1 fallback. Broader tangential lead options
remain a potential improvement. See [remaining stock and arcs](REMAINING_STOCK_AND_ARCS.md)
for sequence, clearance, rest-clearing and simulation guarantees and limits.
Separate ramp spindle speeds and transition-feed preset metadata are not emitted
as mid-operation spindle/phase commands; those preset fields are disabled and
identified as storage-only in the browser editor. There is no universal shop feed/speed
recommendation implied by this audit. Existing machine, fixture, reach and export
preflight checks remain active.
