# Machining geometry and clearing

The illustrated references guided the offset-style roughing, face-to-model
semantics and operation help. The icons are original diagrams. Only implemented
strategies are offered; adaptive engagement control, blend, scallop, steep/shallow
and automatic deburring are not introduced by this change.

## Plane, feature, operation

In **2.5D milling → Recognize**, list model faces or use **Select face** in the
viewport. Selecting an upward horizontal face sets the XY machining level.
**Recognize features** searches openings at this level. The list stays open while
selecting a candidate highlights its outline on the model. Then create a matching
operation from the ribbon, or add its disabled review operation from the list.
The operation's Geometry tab also offers recognized targets.

A side or tilted face needs **Align face to +Z & fit stock**. This creates a new
three-axis setup orientation, fits a blank with top/backing allowances, and records
the selected face. Review fixture placement after this change. Existing linked
operations become stale and cannot export until rebuilt. This is setup rotation,
not simultaneous multi-axis machining or G18/G19 milling.

Recognized geometry retains its opening and bottom Z. Depth passes account for raw stock above the opening. An earlier enabled
whole-stock facing operation can establish a cleared top level and skip those
passes; an opening in the target model alone does not prove that stock is air. Features obstructed by higher model surfaces
are rejected. Recognition remains conservative: vertical circular holes and
rectangular blind pockets without islands/tapers; general surfaces use 3D planning.
Large faceted models show the 500 largest planar faces in the list; picking can
select other planar facets directly.

Facing removes blank material only down to the highest target surface. The default
depth comes from the model. A selected recessed face cannot authorize whole-stock
facing through higher geometry. If the model already touches stock top, add a top
allowance under **Setup → Stock → Top allowance**, then **Fit stock to part**, before facing. Backing allowance is set alongside it. Manual work without an imported target retains its
explicit facing depth.

## 3D offset roughing

Roughing descends at the tool/preset step-down and clears constant-Z offset contours
at each layer. A conservatively dilated target heightfield protects the complete
cutter footprint and every cell edge. Offset spacing bounds diagonal as well as
straight-pass separation. Rings wrap around protected islands. Connected contours
use checked links at cutting depth; disconnected regions retract to clearance.
Finishing remains a separate parallel strategy.

These are conventional offset paths. Entries and links can cut full-width slots;
there is no constant-engagement or adaptive-feed guarantee. Ball-tool roughing uses
a conservative cylindrical footprint; subsequent finishing removes that remaining
stock. Grid step and allowances determine retained material and faceting. The
existing 262,144-cell surface grid, 50-million-work and 200,000-move limits remain.

## Tools and simulation

**New tool** opens a type chooser and appends a separate, uniquely numbered record.
It does not open or edit the previous tool. Existing operation defaults remain
assigned when adding tools to an existing job. The table combines text search
with an exact cutting-tool type filter. New empty jobs can still activate their
newly created tool.

Detailed inspection offers up to **512 cells** on the stock's longest side. Inspection and playback share the **Stock simulation resolution** selector;
Standard/64 is the default. Live
updates retain previously removed stock and replace only changed mesh strips,
with animation-frame pacing and one calculation in flight. Seeking and
rewinding reset stale incremental state at every resolution. The detailed simulation is
bounded to 512² cells and 2 million generated
triangles above 192 cells. Lower detail retains the previous 100,000-triangle cap.
**Settings → Simulation calculation budget** selects Auto, Standard (50 million),
Extended (100 million) or High (200 million) expensive removal evaluations per
update or inspection. Auto chooses 100 million if the browser reports at least
4 GiB memory and four logical processors; browsers without those hints use
50 million. This does not measure current free memory. A separate bounded traversal
guard also remains. The choice also applies to remaining-stock OBJ export.
These are display budgets; import and machining validation are unchanged.
High settings can take longer and need more memory; cancellation remains available
for detailed inspection.

Browser stock simulation requires current generated, nonempty toolpaths. Editing
invalidates them and disables stock simulation until generation succeeds. Inspection
and playback share one resolution control; updates have no fixed timer/FPS cap,
with one worker request in flight and only the latest target queued.
