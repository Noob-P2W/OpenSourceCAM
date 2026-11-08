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
