"""Build a self-contained CAD runtime beside the app; no end-user Python needed.

Requires uv and Python 3.11+ on the build machine only. Run from any directory:
  python scripts/bundle-cad.py --output target
  python scripts/bundle-cad.py --output dist/windows-x64 --exe target/release/opensourcecam.exe
All downloads happen at build time. The installed app runs completely offline.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import hashlib
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
PYTHON_VERSION = "3.12.10"
OCP_VERSION = "7.9.3.1.1"


def copy_licenses(runtime):
    source = ROOT / "licenses/third-party"
    manifest = json.loads((source / "sources.json").read_text(encoding="utf-8"))
    for name, record in manifest.items():
        if hashlib.sha256((source / name).read_bytes()).hexdigest() != record["sha256"]:
            raise SystemExit(f"License snapshot hash mismatch: {name}")
    shutil.copytree(source, runtime / "licenses", dirs_exist_ok=True)


def rust_licenses(output):
    env = os.environ.copy()
    cargo = shutil.which("cargo")
    local_cargo = ROOT / ".tools/cargo/bin/cargo.exe"
    if not cargo and local_cargo.is_file():
        cargo = str(local_cargo)
        env["CARGO_HOME"] = str(ROOT / ".tools/cargo")
        env["RUSTUP_HOME"] = str(ROOT / ".tools/rustup")
    if not cargo:
        raise SystemExit("Cargo is required to collect release dependency notices")
    tree = subprocess.run([cargo, "tree", "--locked", "--offline", "--edges", "normal,build",
                           "--prefix", "none", "--format", "{p}"], cwd=ROOT, env=env,
                          text=True, capture_output=True, check=True).stdout
    crates = set(re.findall(r"^([A-Za-z0-9_-]+) v([^\s]+)", tree, re.MULTILINE))
    roots = [Path(env.get("CARGO_HOME", str(Path.home() / ".cargo"))), ROOT / ".tools/cargo"]
    inventory = []
    for name, version in sorted(crates):
        if name == "opensourcecam":
            continue
        candidates = [p for root in roots for p in (root / "registry/src").glob(f"*/{name}-{version}")]
        if not candidates:
            raise SystemExit(f"Dependency source missing for license collection: {name} {version}")
        source = candidates[0]
        package = tomllib.loads((source / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        destination = output / "licenses/rust" / f"{name}-{version}"
        copied = []
        for file in source.rglob("*"):
            if file.is_file() and any(token in file.name.upper() for token in ("LICENSE", "LICENCE", "COPYING", "NOTICE", "OFL", "UFL")):
                target = destination / file.relative_to(source)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(file, target)
                copied.append(str(file.relative_to(source)))
        inventory.append({"name": name, "version": version, "license": package.get("license"),
                          "source": f"https://crates.io/crates/{name}/{version}", "notices": copied})
    (output / "licenses").mkdir(exist_ok=True)
    (output / "licenses/rust-dependencies.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--archive", type=Path, help="Optional .zip (Windows) or .tar.gz (Linux) output")
    args = parser.parse_args()
    if platform.machine().lower() not in ("amd64", "x86_64") or platform.system() not in ("Windows", "Linux"):
        raise SystemExit("Bundled runtime currently targets Windows x64 and Linux x64")
    uv = shutil.which("uv")
    if not uv:
        raise SystemExit("Build dependency 'uv' is missing: https://docs.astral.sh/uv/getting-started/installation/")
    runtime = args.output.resolve() / "cad-runtime"
    tools = ROOT / ".tools"
    install = tools / "cad-python"
    env = os.environ.copy()
    env["UV_CACHE_DIR"] = str(tools / ("uv-cache" if platform.system() == "Windows" else "linux-uv-cache"))
    env["UV_PYTHON_BIN_DIR"] = str(tools / "cad-python-bin")
    subprocess.run([uv, "--no-config", "python", "install", PYTHON_VERSION, "--install-dir", str(install)], env=env, check=True)
    installations = list(install.glob(f"cpython-{PYTHON_VERSION}-*"))
    expected = "windows" if platform.system() == "Windows" else "linux"
    installations = [p for p in installations if expected in p.name and "x86_64" in p.name]
    if len(installations) != 1:
        raise SystemExit("Expected exactly one matching portable Python installation")
    python_relative = Path("python.exe") if expected == "windows" else Path("bin/python3.12")
    runtime.mkdir(parents=True, exist_ok=True)
    shutil.copytree(installations[0], runtime / "python", dirs_exist_ok=True, symlinks=True)
    python = runtime / "python" / python_relative
    subprocess.run([
        uv, "--no-config", "pip", "install", "--python", str(python),
        "--target", str(runtime / "packages"), "--only-binary=:all:", "--require-hashes",
        "--index-url", "https://pypi.org/simple", "-r", str(ROOT / "scripts/cad-requirements.txt"),
    ], env=env, check=True)
    subprocess.run([str(python), "-I", "-c",
                    "import sys; sys.path.insert(0, sys.argv[1]); from OCP.STEPControl import STEPControl_Reader; from OCP.IGESControl import IGESControl_Reader; print('Bundled STEP/IGES kernel ready')",
                    str(runtime / "packages")], check=True)
    (runtime / "manifest.json").write_text(json.dumps({
        "engine": "OpenCascade / OCP (no VTK)", "package_version": OCP_VERSION,
        "python_version": PYTHON_VERSION, "platform": platform.system(), "architecture": "x86_64",
        "network_required_at_runtime": False,
    }, indent=2) + "\n", encoding="utf-8")
    shutil.copy2(ROOT / "scripts/cad-requirements.txt", runtime)
    shutil.copy2(ROOT / "THIRD_PARTY_NOTICES.md", args.output / "THIRD_PARTY_NOTICES.md")
    copy_licenses(runtime)
    if args.exe:
        rust_licenses(args.output)
        shutil.copy2(args.exe, args.output / args.exe.name)
        for name in ("README.md", "LICENSE"):
            shutil.copy2(ROOT / name, args.output / name)
        for name in ("docs", "examples"):
            shutil.copytree(ROOT / name, args.output / name, dirs_exist_ok=True)
        source = args.output / "source"
        for directory in ("src", "scripts", "tests", "licenses", "examples", "docs", ".github"):
            shutil.copytree(ROOT / directory, source / directory, dirs_exist_ok=True,
                            ignore=shutil.ignore_patterns("__pycache__"))
        for name in ("Cargo.toml", "Cargo.lock", "README.md", "LICENSE", "THIRD_PARTY_NOTICES.md"):
            shutil.copy2(ROOT / name, source / name)
    size = sum(p.stat().st_size for p in runtime.rglob("*") if p.is_file())
    print(f"Bundled runtime: {runtime} ({size / 1024**2:.1f} MiB uncompressed)")
    if args.archive:
        archive = args.archive.resolve()
        archive.parent.mkdir(parents=True, exist_ok=True)
        if archive.name.endswith(".tar.gz"):
            shutil.make_archive(str(archive)[:-7], "gztar", root_dir=args.output.parent, base_dir=args.output.name)
        elif archive.suffix == ".zip":
            # Portable Python intentionally has reproducible epoch timestamps,
            # older than ZIP's 1980 lower bound. Clamp archive dates, not files.
            if archive.is_relative_to(args.output.resolve()):
                raise SystemExit("Put the release archive outside the package directory")
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED,
                                 compresslevel=6, strict_timestamps=False) as bundle:
                for file in sorted(args.output.resolve().rglob("*")):
                    if file.is_file():
                        bundle.write(file, file.relative_to(args.output.resolve().parent))
        else:
            raise SystemExit("Archive must end with .zip or .tar.gz")
        print(f"Release archive: {archive} ({archive.stat().st_size/1024**2:.1f} MiB)")


if __name__ == "__main__":
    main()
