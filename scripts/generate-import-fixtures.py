"""Regenerate our original CAD test assets using the bundled open-source kernel.

Run with target/cad-runtime/python/python.exe (Windows) or python/bin/python3.12.
"""
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "target/cad-runtime/packages"))

from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder
from OCP.IFSelect import IFSelect_RetDone
from OCP.IGESControl import IGESControl_Writer
from OCP.Interface import Interface_Static
from OCP.STEPControl import STEPControl_Writer, STEPControl_AsIs
from OCP.gp import gp_Pnt, gp_Ax2, gp_Dir

output = ROOT / "examples/models"
output.mkdir(parents=True, exist_ok=True)
shape = BRepPrimAPI_MakeBox(80, 50, 12).Shape()
pocket = BRepPrimAPI_MakeBox(gp_Pnt(20, 15, 7), 40, 20, 6).Shape()
shape = BRepAlgoAPI_Cut(shape, pocket).Shape()
for x, y in [(10, 10), (70, 10), (10, 40), (70, 40)]:
    axes = gp_Ax2(gp_Pnt(x, y, 0), gp_Dir(0, 0, 1))
    shape = BRepAlgoAPI_Cut(shape, BRepPrimAPI_MakeCylinder(axes, 3, 12).Shape()).Shape()


def step(model, path, units="MM"):
    writer = STEPControl_Writer()
    if not Interface_Static.SetCVal_s("write.step.unit", units):
        raise RuntimeError("Could not configure STEP export units")
    writer.Transfer(model, STEPControl_AsIs)
    if writer.Write(str(path)) != IFSelect_RetDone:
        raise RuntimeError("STEP fixture export failed")


step(shape, output / "mount.step")
iges = IGESControl_Writer("MM", 0)
iges.AddShape(shape)
iges.ComputeModel()
if not iges.Write(str(output / "mount.iges")):
    raise RuntimeError("IGES fixture export failed")

# 1 x 2 x 0.5 inches physically, exported with inch file units.
inch_box = BRepPrimAPI_MakeBox(25.4, 50.8, 12.7).Shape()
step(inch_box, output / "inch-box.step", "INCH")
Interface_Static.SetCVal_s("write.step.unit", "MM")

# Import/tessellate via the exact same worker used by Rust, then write mesh examples.
import importlib.util
spec = importlib.util.spec_from_file_location("bridge", ROOT / "scripts/cad_bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
bridge.convert({"format": "step", "source": str(output / "mount.step"),
                "output": str(output / "mount.stl"), "max_triangles": 100_000})
data = (output / "mount.stl").read_bytes()
count = struct.unpack_from("<I", data, 80)[0]
lines = ["# OpenSourceCAM original fixture, millimetres", "o mounting_plate"]
for i in range(count):
    values = struct.unpack_from("<12fH", data, 84 + i * 50)
    for j in (3, 6, 9):
        lines.append("v " + " ".join(str(v) for v in values[j:j+3]))
    lines.append(f"f {i*3+1} {i*3+2} {i*3+3}")
(output / "mount.obj").write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"Generated mount STEP / IGES / STL / OBJ with {count} mesh triangles")
