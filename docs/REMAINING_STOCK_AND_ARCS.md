# Remaining stock, local clearance and circular interpolation

The generator tracks remaining stock in operation order from the original blank.
Disabled and later operations cannot establish cleared stock. A new generation
rebuilds this state, including each operation's actual tool and depth.

## Clearance and rest clearing

Set **Remaining-stock clearance · mm** in an operation's **Heights & accuracy**
tab to allow local rapid moves above the remaining material. With Optimized milling, a blank uses a proven local 0.5 mm gap. Climb/conventional
mode with a blank field retains full setup clearance. The gap must be at least 0.05 mm and no greater than
the operation clearance. Cutting tool, shaft and defined holder envelopes all
contribute. Fixture surface projections use a cached spatial tree, including
hidden fixtures. Full clearance is retained for real tool changes and program end; consecutive
same-tool optimized operations may retain proven local clearance. Export retains the existing swept fixture and machine checks.

A bounded grid stores an upper height for every entire cell, rather than assuming
a cell is empty after its centre is cut. Flat, ball and conical cutter profiles
share the simulation kernel. Sloping sweeps evaluate cutter coverage along the
move; the deepest endpoint is never stamped over the entire sweep. Circular chord
tolerance and posted coordinate rounding are reserved in removal certificates.
The sequence grid has at most 262,144 cells and a 50-million-visit budget.
Uncertainty/exhaustion means material remains: local links revert to full clearance
and automatic preparation checks fail rather than infer air.

Bore/circular/thread wall cycles check preceding pilot/surrounding clearance.
Planar finishing checks preceding stock height against permitted axial engagement.
Buried engraving/chamfer references check exposure along the whole chain, including
its closing segment. Explicit externally prepared stock remains an operator
assertion for these cycles; it does **not** insert fictional cuts into the stock
state or authorize low rapids. A whole-blank facing pass also proves exposure.

Linked outside clearing projects the complete model silhouette, retaining raised
features at every depth, and floods only cutter-accessible regions connected to
the stock exterior. It follows irregular silhouettes and does not clear enclosed
holes/pockets. Its XY offset chains are calculated once per operation and reused
at each depth. Numeric outside clearing retains its explicitly specified rectangle.
2D offset clearing and linked outside clearing omit chains whose complete swept
footprint is already proven air. Partially clear chains retain their cuts; this is
conservative rest clearing, not an engagement-controlled adaptive strategy.

## Simulation

The browser defaults to **Conservative remaining stock**. Rectangular blanks show
a continuous conservative upper envelope, with shared vertices taking the maximum
of adjacent whole-cell heights and flat strips merged to reduce mesh size. Raise
resolution to reduce conservative residual material. Playback, detailed inspection and downloaded stock OBJ use
the same selected mode. Seeking, editing, changing resolution or changing mode
invalidates the incremental state. Work/mesh limits still return explicit errors.

The sampled mode remains available. For oriented boxes and cylinders, the mesh
retains sampled physical columns; the separate conservative volume bound uses the
blank's enclosing box. Target gouge/excess comparisons remain cell-centre samples,
even with a conservative display, and cannot certify an absence of gouging.

This is a 2.5D upper-envelope model, not exact volumetric CSG. Undercuts, trapped
voids, side access and thread grooves are not represented. Thread-removal
simulation remains unavailable; thread moves never certify removed material for
subsequent clearance decisions. Actual tool dimensions, mesh accuracy, machine
configuration and workholding still determine physical machining results.

## G2/G3

Generic, GRBL, Mach3 and LinuxCNC posts preserve analytical circles and cylindrical
helices from the generators. They emit G17 XY arcs with explicit G91.1 incremental
I/J centres, absolute XYZ endpoints, and the applicable cutting/ramp/lead feed.
Full turns are split into spans of at most 180 degrees. Fractional turns retain
their axial pitch and handedness. Arc emission never crosses a tool, operation or
feed boundary. Initial XY positioning and manual-change centre modes are explicit.

The internal linear samples remain available for preview, stock removal and
collision checks. True-curve cardinal extrema and endpoint/cardinal tangent feeds
receive additional machine checks; inflated fixture envelopes contain the true
arc between samples. Uncertain arc preflight keeps G1 output. Very small rounded
entries or GRBL arc lines exceeding its buffer also keep G1 samples. Arbitrary
polygon contours, conical spirals and conservative circumscribed outside corners
are never blindly fitted to circles. **Motion output** is available in Active
machine and in the **Postprocessor & changer** tab of Machine Library records.
Records retain the selected format when applied to another job.

- **G1 · Linear moves** emits bounded linear feed samples and omits G91.1.
- **G2/G3 · Circular interpolation** is the default and preserves verified
  circles and helices on the existing milling posts.
- **G5 · Cubic spline (LinuxCNC)** posts bounded XY cubic spans for eligible
  flat arcs. Helices and ineligible cubics use verified circular output. The
  same swept-arc preflight applies, and linear samples cover the cases that
  cannot pass it. [LinuxCNC G5 reference](https://linuxcnc.org/docs/stable/html/gcode/g-code.html#gcode:g5)

Existing projects and machine records with the old linear-only flag open in G1
mode. Selecting a motion format explicitly takes precedence over the flag.

Optimized, Climb and Conventional planar clearing use tangent circular fillets
where the grid contains
the entire triangle between the original corner and both tangent endpoints.
The triangle certificate covers the true arc and its linear samples. Neighboring
fillets share the available line length, and pass spacing reserves their maximum
departure from the grid route. Protected or tightly spaced turns keep their
original geometry. The controller still determines acceleration through the
resulting arcs and how it stops at any remaining sharp corners.

## Primary references

- [Autodesk: stock transfer for rest machining](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/MFG-REST-MACHINING-SETUP-STOCK-TRANSFER.htm)
  and [reducing 3D offset air cutting](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/REDUCE-AIR-CUTTING-POCKET.htm):
  use preceding remaining material, rather than the target model alone, to reason
  about rest machining and linking. This implementation uses its own bounded
  conservative kernel; it does not reproduce Fusion's algorithms.
- [LinuxCNC G-code reference](https://linuxcnc.org/docs/html/gcode/g-code.html#gcode:G2-G3)
  and [arc centre modes](https://linuxcnc.org/docs/html/gcode/g-code.html#gcode:G90.1-G91.1):
  centre-format circular/helical motion, plane, direction, endpoint/centre modes
  and precision are explicit contracts.
- [Haas circular/helical interpolation](https://www.haascnc.com/service/codes-settings.type=gcode.machine=mill.value=G03.html):
  incremental centre offsets and axial travel per revolution.
- [GRBL supported G-codes](https://github.com/gnea/grbl/wiki): XY arcs and helical
  interpolation; the supported incremental centre convention was also checked
  against the upstream parser. No implementation code was copied.

Regression checks independently reconstruct posted arc geometry and sample prior
cutter sweeps, test disabled/reordered/shallow preparation, mode/seek resets,
fixture-height bounds, resource exhaustion, and an original closed annular model
whose exterior corners must be cleared while its hole remains untouched.
