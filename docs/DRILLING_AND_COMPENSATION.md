# Drilling cycles and milling cutter compensation

Standard drilling and full-retract peck drilling generate expanded G0/G1 motion.
This lets Generic, GRBL, Mach3 and LinuxCNC use the same checked paths, playback
and stock removal instead of relying on controller-specific canned-cycle support.

## Drilling

- **Drill hole / continuous feed** (`drill_once`, G81-equivalent): retract to setup
  clearance, position XY, rapid through air to the R plane, feed axially to the
  programmed drill-tip depth, then retract to clearance.
- **Peck drilling** (`drill`, G83-equivalent): feed each Q increment (the peck-depth
  override or tool preset), fully retract to R for chip clearing, rapid back to
  above the previously drilled bottom and feed the next peck. The last peck is
  shortened to finish exactly at the programmed tip depth. Default re-entry
  clearance is 0.254 mm and is independently editable. Rapid re-entry never goes
  below the previously cleared bottom.

R must be positive above stock top Z0 and no higher than operation clearance. XY
travel uses full clearance; the operation ends at the job's travel clearance.
A library drilling operation requires a drill definition. Per-revolution feed
presets drive axial feed and follow spindle overrides; an explicit drilling feed
override takes precedence. Feeds, speed, depth, workholding, travel and reach still
pass the shared checks. Peck and move limits remain enforced. Standard drilling
ignores peck count because it has one axial feed.

Depth locates the drill tip, not the end of the full-diameter cylindrical hole.
A pointed drill must travel further for a full-diameter through hole. Recognized
through-hole conversion already accounts for point length and requires enough
blank backing; a manually specified operation uses the explicit tip depth. Blind
flat-bottom geometry requires an appropriate milling operation. Bottom dwell,
G73 chip-break cycles, tapping and automatic material-specific cutting recipes
are not implemented here. Existing nonzero dwell presets remain blocked.

## Milling

Pockets and inside profiles offset their cutter center inward by tool radius plus
radial allowance. Outside profiles offset outward by the same amount. Clockwise
and counterclockwise reverse traversal without switching the retained side.
Raster direction also retains these same compensated bounds. An end mill leaves
its radius at sharp internal corners.

Outside profiles and exterior stock clearing round external corners with a tangent
polygon around the required offset circle. Every segment stays outside the circle,
so chord approximation does not gouge the corner. The maximum outward deviation
is bounded by the operation's path tolerance (default 0.01 mm). This replaces square
centerline turns that could leave excess material around the outside corner.
Allowance and final passes retain the same offset. Output starts with G40 because
these are already compensated cutter-center coordinates; G41/G42 must not be added
on top of them. Tool length and work offsets remain separate controller setup data.
The first exterior clearing contour encloses the blank's diagonal corners so the
rounded clearing passes also remove material there.

## Sources and verification

The cycle review used the official LinuxCNC documentation and GRBL sources:

- [LinuxCNC G81 drilling](https://linuxcnc.org/docs/stable/html/gcode/g-code.html#gcode:g81)
- [LinuxCNC G83 peck drilling](https://linuxcnc.org/docs/stable/html/gcode/g-code.html#gcode:g83)
- [Official documentation source consulted](https://github.com/LinuxCNC/linuxcnc/blob/master/docs/src/gcode/g-code.adoc)
- [GRBL supported-code scope](https://github.com/gnea/grbl#readme), which notes its lack of most canned cycles.

Regression tests cover single feed, exact peck endpoints, R/clearance travel,
re-entry into cleared material, spindle/feed overrides, tool suitability, resource
limits and expanded output for all four implemented milling posts. Offset tests
check both directions and raster reversals, retained sides, corner distance and
absence of double controller compensation. This is software verification, not
physical cutting verification.
