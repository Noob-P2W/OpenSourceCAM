"""Private bundled OCCT worker. Called by Rust with file paths, never shell code.

Only the bundled OCP packages are added to the isolated interpreter's path.
The exchange source is loaded as CAD data, not imported as a Python module.
"""
import json
import math
from pathlib import Path
import struct
import sys


def convert(request):
    from OCP.BRep import BRep_Tool
    from OCP.BRepMesh import BRepMesh_IncrementalMesh
    from OCP.IFSelect import IFSelect_RetDone
    from OCP.Interface import Interface_Static
    from OCP.TopAbs import TopAbs_FACE, TopAbs_REVERSED
    from OCP.TopExp import TopExp_Explorer
    from OCP.TopLoc import TopLoc_Location
    from OCP.TopoDS import TopoDS

    if request["format"] == "step":
        from OCP.STEPControl import STEPControl_Reader
        reader = STEPControl_Reader()
    else:
        from OCP.IGESControl import IGESControl_Reader
        reader = IGESControl_Reader()
    # OCCT's internal geometry length unit for both exchange readers.
    Interface_Static.SetCVal_s("xstep.cascade.unit", "MM")
    if reader.ReadFile(request["source"]) != IFSelect_RetDone:
        raise ValueError("The CAD kernel could not read the exchange file")
    if reader.TransferRoots() == 0:
        raise ValueError("The file contains no transferable CAD shapes")
    shape = reader.OneShape()
    if shape.IsNull():
        raise ValueError("The CAD shape is empty")
    if request["format"] == "iges":
        # IGES often transfers independent faces. Share coincident edges before
        # meshing so neighbouring faces receive compatible boundary subdivisions.
        # Keep the sewing tolerance below the Rust vertex welding tolerance;
        # do not fill gaps or relax the recognizer's closed-surface requirement.
        from OCP.BRepBuilderAPI import BRepBuilderAPI_Sewing
        sewing = BRepBuilderAPI_Sewing(1e-5)
        sewing.Add(shape)
        sewing.Perform()
        sewn = sewing.SewedShape()
        if not sewn.IsNull():
            shape = sewn
    mesher = BRepMesh_IncrementalMesh(shape, 0.1, False, 0.3, False)
    if not mesher.IsDone():
        raise ValueError("The CAD kernel could not tessellate this shape")
    explorer = TopExp_Explorer(shape, TopAbs_FACE)
    facets = bytearray()
    count = 0
    while explorer.More():
        face = TopoDS.Face_s(explorer.Current())
        location = TopLoc_Location()
        triangulation = BRep_Tool.Triangulation_s(face, location)
        if triangulation is None:
            raise ValueError("A CAD face has no surface triangulation")
        transform = location.Transformation()
        for index in range(1, triangulation.NbTriangles() + 1):
            indices = list(triangulation.Triangle(index).Get())
            if face.Orientation() == TopAbs_REVERSED:
                indices[1], indices[2] = indices[2], indices[1]
            points = [triangulation.Node(i).Transformed(transform) for i in indices]
            coords = [value for p in points for value in (p.X(), p.Y(), p.Z())]
            if not all(math.isfinite(v) and abs(v) <= 1_000_000 for v in coords):
                raise ValueError("CAD coordinates exceed the supported range")
            count += 1
            if count > request["max_triangles"]:
                raise ValueError("CAD tessellation exceeds 100,000 triangles")
            facets.extend(struct.pack("<12fH", 0.0, 0.0, 0.0, *coords, 0))
        explorer.Next()
    if count == 0:
        raise ValueError("No surfaces found; wire-only CAD models are not supported yet")
    header = b"OpenSourceCAM bundled OCCT tessellation".ljust(80, b"\0")
    Path(request["output"]).write_bytes(header + struct.pack("<I", count) + facets)


if __name__ == "__main__":
    request = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    sys.path.insert(0, str(Path(sys.argv[2]).resolve()))
    try:
        convert(request)
    except Exception as error:
        Path(request["error"]).write_text(str(error), encoding="utf-8")
        raise SystemExit(1) from error
