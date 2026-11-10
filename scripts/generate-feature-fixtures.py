"""Original recognition regression geometry; run with the bundled CAD interpreter."""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "target/cad-runtime/packages"))
sys.path.insert(0, str(ROOT / "scripts"))
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder
from OCP.STEPControl import STEPControl_Writer, STEPControl_AsIs
from OCP.IFSelect import IFSelect_RetDone
from OCP.gp import gp_Pnt, gp_Ax2, gp_Dir
import cad_bridge

output = ROOT / "tests/fixtures"
output.mkdir(exist_ok=True)

def cylinder(x, y, z, radius, height):
    return BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x,y,z),gp_Dir(0,0,1)),radius,height).Shape()

def write(shape, name):
    step = output / (name + ".step")
    writer = STEPControl_Writer()
    writer.Transfer(shape, STEPControl_AsIs)
    if writer.Write(str(step)) != IFSelect_RetDone:
        raise RuntimeError("Cannot write feature fixture")
    cad_bridge.convert({"format":"step", "source":str(step), "output":str(output/(name+".stl")), "max_triangles":100000})

base = BRepPrimAPI_MakeBox(80,50,12).Shape()
counterbore = BRepAlgoAPI_Cut(base,cylinder(10,10,0,3,12)).Shape()
counterbore = BRepAlgoAPI_Cut(counterbore,cylinder(10,10,7,5,6)).Shape()
pocket = BRepPrimAPI_MakeBox(gp_Pnt(20,15,7),40,20,6).Shape()
island = BRepPrimAPI_MakeBox(gp_Pnt(35,20,6),10,10,8).Shape()
cut = BRepAlgoAPI_Cut(pocket,island).Shape()
write(BRepAlgoAPI_Cut(counterbore,cut).Shape(),"features-unsupported")
write(BRepAlgoAPI_Cut(base,cylinder(30,25,4,3,9)).Shape(),"features-blind-hole")

# Original fixtures for detailed recognition, with actual chamfer and fillet surfaces.
from OCP.BRepFilletAPI import BRepFilletAPI_MakeChamfer, BRepFilletAPI_MakeFillet
from OCP.BRepAdaptor import BRepAdaptor_Curve
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_EDGE
from OCP.TopoDS import TopoDS
from OCP.GeomAbs import GeomAbs_Circle
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCone
from OCP.BRepOffsetAPI import BRepOffsetAPI_ThruSections
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakePolygon

def rim(shape, x):
    explorer=TopExp_Explorer(shape,TopAbs_EDGE)
    while explorer.More():
        e=TopoDS.Edge_s(explorer.Current()); c=BRepAdaptor_Curve(e)
        if c.GetType()==GeomAbs_Circle and abs(c.Circle().Location().Z()-12)<1e-6 and abs(c.Circle().Location().X()-x)<1e-6:
            return e
        explorer.Next()
    raise RuntimeError('Missing hole rim')

plain=BRepAlgoAPI_Cut(base,cylinder(20,25,-1,3,14)).Shape()
chamfer=BRepFilletAPI_MakeChamfer(plain);chamfer.Add(1.,rim(plain,20));chamfer.Build()
if not chamfer.IsDone(): raise RuntimeError('Chamfer fixture failed')
write(chamfer.Shape(),'recognition-chamfer')
fillet=BRepFilletAPI_MakeFillet(plain);fillet.Add(0.8,rim(plain,20));fillet.Build()
if not fillet.IsDone(): raise RuntimeError('Fillet fixture failed')
write(fillet.Shape(),'recognition-fillet')

cone=BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(60,25,-1),gp_Dir(0,0,1)),2.,5.5,14.).Shape()
tapers=BRepAlgoAPI_Cut(base,cone).Shape()
loft=BRepOffsetAPI_ThruSections(True,True)
for lo,hi,z in [(12,22,5),(10,24,13)]:
    wire=BRepBuilderAPI_MakePolygon()
    for x,y in [(lo,lo),(hi,lo),(hi,hi),(lo,hi)]:wire.Add(gp_Pnt(x,y,z))
    wire.Close();loft.AddWire(wire.Wire())
loft.Build()
tapers=BRepAlgoAPI_Cut(tapers,loft.Shape()).Shape()
write(tapers,'recognition-tapers')
open_cut=BRepPrimAPI_MakeBox(gp_Pnt(-1,18,6),26,12,8).Shape()
write(BRepAlgoAPI_Cut(base,open_cut).Shape(),'recognition-open-pocket')
