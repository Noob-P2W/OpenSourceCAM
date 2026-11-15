# Grouped 2D milling

The browser ribbon groups operations by the machining task. The operation editor
keeps one shared Geometry / Tool & preset / Passes / Feeds / Heights workflow.
Existing saved operations retain their strategy and raster/plunge defaults.
New rectangular pockets default to offset clearing. No sketch editor is included;
geometry comes from imported models or numeric coordinates.

| Section | Command | Behavior |
| --- | --- | --- |
| Roughing | Face | Whole-blank facing down to the highest target surface |
| Roughing | Pocket roughing | Rectangular/circular clearing with offset or raster passes |
| Roughing | Outside roughing | Offset passes outside the full retained workpiece envelope |
| Roughing | Slot | Cutter-width centerline passes; a capsule slot's entry chamfer is a separate operation |
| Finishing | Pocket contour / Outside contour | Compensated wall finishing on picked edge chains or numeric boundaries |
| Finishing | Surface smoothing | Raster finish a selected horizontal planar face, respecting its boundary and islands |
| Finishing | Hole milling | Helical bore, circular wall passes or thread milling |
| Finishing | Chamfer | Measured V-cutter finishing, with separate width, angle and tip penetration |
| Other | Engrave | V-cutter centerline machining |

Trace and the previous smooth-loop roughing cycle remain loadable for saved jobs.
They are removed from operation creation.
Drilling remains its own ribbon category, with continuous and peck cycles.

## Geometry and heights

Use **Pick on model** before choosing a cutter, or enter numeric geometry.
Pocket and roughing picks use supported circular/rectangular recognition. Slot
picks derive a straight centerline from a rectangular feature. Contour and Chamfer pick machining edges, with single, boundary-chain and
connected-edge modes. Engrave follows face-boundary centerlines. Surface smoothing
selects a horizontal planar face with its boundary and islands. These selections
use imported tessellation rather than a general analytic CAD edge network. Preview and
review the resulting chain. Geometry links become stale after model placement or
orientation changes; unlinking permits manual editing.

Numeric chains use one `X Y Z` point per line in work coordinates. Engrave and
Chamfer subtract tip penetration from each point's Z. Contours use tool-tip height
offset; Slot uses depth below stock top. Leave Top depth at zero for model-height
chains. Slot chains must be at Z=0; Chamfer and compensated Contour chains must be
flat. Trace and Engrave permit varying Z. Open chains do not acquire a closing
segment; Closed chain explicitly adds it.

Contour compensation is bounded to 2,000 vertices, uses mitered corners and
rejects collapsed, crossing, reversing or excessively sharp offsets. Closed-chain
Inside/Outside is independent of winding. For an open chain, Inside means the left
side and Outside means the right side of the selected chain. Milling direction
changes travel order while preserving the compensated side; picked material
boundaries also supply their retained side. This is not a general polygon
boolean/offset engine. Use a smaller cutter or prepared simpler geometry when an
offset is rejected. Pockets remain rectangular/circular; arbitrary pockets,
islands and freeform stock clearing use the existing 3D strategies.

## Clearing and entries

Rectangular Pocket offers offset contours or parallel raster. Offset spacing
bounds diagonal separation as well as straight separation, with a center-out
sequence and a final wall pass. Circular Pocket clears center-out rings. Plunge,
ramp and helix entries are available for both pocket shapes. Ramp/helix descent
is limited to three degrees, remains inside the compensated region, and respects
the axial plunge-feed limit. A sufficiently large region is required. These entry
choices do not authorize using a non-center-cutting cutter.

Outside roughing uses a retained envelope enclosing the complete imported part,
including models with internal features. It clears outside that envelope and
leaves internal pockets alone. It does not silently substitute a smaller numeric
rectangle when a complex model lacks a recognized rectangular exterior. Curved
outside corners can retain stock within the envelope for a subsequent finishing
operation; this is not silhouette-following adaptive roughing.

## Bore, circular and thread cycles

Wall finishing and thread cycles require an explicit **already cleared** checkbox.
It is the user's assertion that the bore or surrounding boss stock has been
cleared, not an automatic preceding roughing operation. Include preceding
clearing operations when simulating removal from a solid blank.

Helical Bore descends continuously at the stepdown per revolution; Circular wall
passes descend between separate rings. Internal walls can taper inward with
increasing depth. External bosses and thread cycles currently require zero taper.
The cutter must fit the smallest bore diameter. Chord tolerance bounds circular
segmentation; final wall circles, optional spring passes and radial exits retain
the selected inside/outside compensation. External helical exits follow the
actual endpoint, including fractional final turns.

Thread milling requires a measured thread mill. Operation pitch must match its
library pitch; radial thread depth must fit tooth height and neck clearance.
Clockwise descending produces a right-hand helix; counterclockwise descending
produces a left-hand helix. The cycle uses a single radial thread-depth pass and
the measured cutter profile; automatic thread standards, multi-start threads,
conical threads and automatic radial roughing passes are not implemented. Repeat
passes require another operation. Review cutter suitability and lead-in/out
clearance, particularly for blind threads.

The sampled stock heightfield cannot represent thread grooves. Projects containing
enabled thread milling can generate paths and checked NC, but stock simulation
is explicitly unavailable. The browser reports this reason without discarding
the valid path. V/chamfer simulation includes the measured flat tip and cone
angle; it remains a sampled approximation.

## Limits and verification

All cycles use shared Rust generation, feed/spindle overrides, machine-travel
checks, fixture preflight and checked NC export. Descending feed moves are limited
by the resolved axial plunge feed. The 200,000-move / 2,000-layer resource limits,
finite-coordinate validation and cancellation remain in force. Chain contours do
not establish complete model-gouge avoidance: users must review the selected side,
heights, clearance and retained geometry. No physical machining validation or
complete collision certification is implied.

Rust regression tests cover each cycle, radius compensation, fractional external
helix exits, pitch/handedness, axial feed, invalid geometry/tools and bounded pocket
entries. Browser tests exercise the grouped controls, all new cycle saves and path
generation, V/thread cutter assignments and circular shape inference.


## Finishing height and milling terms

Wall finishing uses a tool-tip offset from the selected edge. Zero follows its
height exactly; positive values lift the tip. For an edge at Z=-10 mm, offset
+0.5 mm places the tip at Z=-9.5 mm. Negative offsets deliberately cut deeper.
Surface smoothing uses the same convention from the selected face, defaults to
zero and requires confirmation that the face was rough-machined. Its paths are
limited to regions where the complete cutter footprint fits, including island
clearance. A matching flat end mill is required. It is not freeform 3D finishing.

Climb/conventional milling controls feed order relative to spindle rotation and
the retained material. Geometric compensation remains on the selected side when
feed order reverses. Clockwise/counterclockwise identifies spindle rotation
(M3/M4); thread hand remains right/left independently. Full-width slots and raster
face passes use entry and raster-direction controls.

## Picking and chamfered slots

Hover previews the actual available face or machining edge. Edge selection offers
Single edge, Boundary chain, or Connected edges. Connected edges may be added at
either end and are committed with Finish selection; disconnected edges are rejected.
Horizontal machining boundaries are identified from adjacent mesh surfaces rather
than treating a picked face as an edge. Straight collinear tessellation segments
are combined. Smooth/freeform CAD curves still depend on imported tessellation.

Straight capsule slots with vertical walls and a planar entry chamfer are split
into standard Slot and Chamfer geometry. The measured vertical-wall width drives
slotting; entry width and height supply chamfer width and angle. The chamfer cutter
must match the measured included angle. For example, the local test object has a
6 mm slot and a 0.5 mm chamfer at 45 degrees to the face. These do not require a
3D roughing operation. Curved transitions, genuine taper and overhangs remain
separate geometry and are not flattened by the slot cycle. Automatic slot fitting
currently requires its straight axis to align with work X or Y.
