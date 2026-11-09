# Feature recognition

Version 0.9 connects recognition to actual enabled machining paths and a complete
supported-part stock workflow. See [Machining from stock](MACHINING_FROM_STOCK.md)
for the current workflow, source links, drill-point treatment and geometry limits.

Use **Features → Recognize model** to inspect and highlight individual candidates,
then choose a suitable library tool/preset and click **Generate feature toolpath**.
The operation is enabled and linked to the current model and placement. Smaller
flat end mills can interpolate round holes. Blind flat floors require an end mill;
matching drills can program through holes when sufficient bottom allowance exists.

Recognition first checks exact shared-vertex topology, preserving short edges and
small valid triangles. If exact topology has open seams, it retries the existing
0.0001 mm quantized weld for exchange meshes; the result must still pass all
closed-surface, orientation and degeneracy checks. No faces are removed.

Recognition checks closed, oriented topology, complete wall profiles and supported
floor/end boundaries. Only top-access vertical circular holes and axis-aligned
rectangular blind pockets are currently supported. Fine polygons may be inferred
as circles; check the intended design dimensions. Tapers, counterbores, islands and
intersecting openings are unsupported. The complete stock planner additionally
requires one rectangular prismatic exterior and accounts for every surface, rather
than machining an arbitrary model's bounding rectangle.

STL, OBJ, STEP and IGES use the same Rust mesh recognizer. The bundled CAD adapter
sews IGES faces before tessellation so coincident exchange surfaces share topology.
The CAD runtime remains fully bundled and does not require a separate installation.

Geometry links prevent export after model/placement or linked-coordinate changes.
Rebuild the operation, or explicitly unlink it for manual programming. Cutting-data
and pass overrides remain editable. Recognition and preview are approximations;
review dimensions, workholding and the posted program before physical machining.
