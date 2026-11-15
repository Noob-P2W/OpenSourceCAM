# Grouped 2D milling

The browser ribbon groups operations by the machining task. The operation editor
keeps one shared Geometry / Tool & preset / Passes / Feeds / Heights workflow.
Existing saved operations retain their strategy and raster/plunge defaults.
New rectangular pockets default to offset clearing. No sketch editor is included;
geometry comes from imported models or numeric coordinates.

| Command | Choices and behavior |
| --- | --- |
| Face | Existing whole-blank facing, limited to the highest target surface |
| Pocket | Rectangular or circular clearing; a picked hole selects Circular automatically |
| Roughing | Smooth loops (trochoidal) in a rectangle, or existing outside offset clearing |
| Contour | Inside/outside rectangle or planar edge chain, with cutter-radius compensation |
| Slot | Successive depth passes along a centerline, with a cutter matching the slot width |
| Trace | Tool-center XYZ chain, including varying Z, without lateral compensation |
| Hole milling | Helical bore, separate circular wall passes, or thread milling; internal bore or external boss |
| Engrave | XYZ centerline with a measured V/chamfer cutter; penetration sets groove width |
| Chamfer | Flat edge centerline with a measured V/chamfer cutter; penetration sets chamfer width |

Drilling remains its own ribbon category, with continuous and peck cycles.

## Geometry and heights

Use **Pick on model** before choosing a cutter, or enter numeric geometry.
Pocket and roughing picks use supported circular/rectangular recognition. Slot
picks derive a straight centerline from a rectangular feature. Trace, Contour,
Engrave and Chamfer picks select a boundary loop of the clicked model face,
choosing the loop nearest the click. These are tessellated face boundaries, not
analytic CAD curves or an automatic arbitrary-edge chain solver. Preview and
review the resulting chain. Geometry links become stale after model placement or
orientation changes; unlinking permits manual editing.

Numeric chains use one `X Y Z` point per line in work coordinates. The penetration
value is subtracted from each point's Z. Leave Top depth at zero for model-height
chains. Slot chains must be at Z=0; Chamfer and compensated Contour chains must be
flat. Trace and Engrave permit varying Z. Open chains do not acquire a closing
segment; Closed chain explicitly adds it.

Contour compensation is bounded to 2,000 vertices, uses mitered corners and
rejects collapsed, crossing, reversing or excessively sharp offsets. Closed-chain
Inside/Outside is independent of winding. For an open chain, Inside means the left
side and Outside means the right side in the chosen travel direction. Reversing
an open chain therefore also changes that side. This is not a general polygon
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

Smooth roughing uses translating circular trochoidal rows and a boundary pass at
each depth. It requires a rectangle at least two tool diameters wide and at most
40% forward advance. This is **not constant-engagement adaptive clearing**: stock
engagement is not solved, and entries, links and boundary passes can encounter
full cutter width. It does not clear around islands or implement tapered walls.
Pocket and Slot clearing account for stock above the selected opening; a hole in
the target model does not establish already removed stock.

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


The 2D ribbon is now ordered Roughing, Finishing, Other. Pocket roughing and
Smooth pocket roughing are separate from Pocket contour; Outside contour opens
an outside finishing operation. Model edge selection offers a single boundary
segment or its boundary loop. Chamfer width and tip depth are separate: the Rust
engine offsets the tip so its measured cone intersects the edge by the requested
width, rejecting insufficient penetration. Cutter side is inferred from the picked
face and remains editable. Trace uses wall compensation for picked flat boundaries;
intentional tool-center and varying-Z traces require Offset cutter from wall off.
The edge picker follows tessellated boundaries of the hit face, not an analytic CAD
edge network across multiple faces. Inspect the chain before generating.
Nonrectangular straight-wall cavities are labelled Pocket and remain inspection
candidates; rectangular Pocket clearing is not authorized to erase their shape.
