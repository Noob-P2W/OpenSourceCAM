"""Generate our original single-file, four-component vise assembly regression.

Run with the bundled CAD Python and its packages on PYTHONPATH.
"""
from pathlib import Path
from OCP.BRep import BRep_Builder
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPCAFControl import STEPCAFControl_Writer
from OCP.STEPControl import STEPControl_AsIs
from OCP.TCollection import TCollection_ExtendedString
from OCP.TDataStd import TDataStd_Name
from OCP.TDocStd import TDocStd_Document
from OCP.TopoDS import TopoDS_Compound
from OCP.XCAFDoc import XCAFDoc_DocumentTool
from OCP.gp import gp_Pnt

root = Path(__file__).resolve().parent.parent
doc = TDocStd_Document(TCollection_ExtendedString("BinXCAF"))
tool = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())
builder = BRep_Builder()
assembly = TopoDS_Compound()
builder.MakeCompound(assembly)
shapes = []
# Fixed and moving jaw corners touch the base: identities must survive contact.
for name, lo, size in [
    ("Vise base", (-10, -20, -37), (140, 130, 10)),
    ("Fixed jaw", (-10, -20, -27), (140, 20, 20)),
    ("Movable jaw", (-10, 100, -27), (140, 10, 20)),
    ("Movable jaw insert", (-10, 90, -27), (140, 10, 20)),
]:
    shape = BRepPrimAPI_MakeBox(gp_Pnt(*lo), *size).Shape()
    shapes.append((name, shape))
    builder.Add(assembly, shape)
label = tool.AddShape(assembly, True)
TDataStd_Name.Set_s(label, TCollection_ExtendedString("Original test vise"))
for name, shape in shapes:
    child = tool.FindShape(shape, False)
    TDataStd_Name.Set_s(child, TCollection_ExtendedString(name))
writer = STEPCAFControl_Writer()
writer.SetNameMode(True)
if not writer.Transfer(doc, STEPControl_AsIs):
    raise RuntimeError("STEP assembly transfer failed")
if writer.Write(str(root / "tests/fixtures/vise-assembly.step")) != IFSelect_RetDone:
    raise RuntimeError("STEP assembly export failed")
