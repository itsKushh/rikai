"""Rassembler les déclarations de licence des dépendances résolues pour la distribution."""

import importlib.metadata as md
import json
from pathlib import Path
import shutil
import subprocess

dest = Path("dist/notices")
dest.mkdir(parents=True, exist_ok=True)
shutil.copyfile("assets/inter/OFL.txt", dest / "Inter-OFL.txt")
meta = json.loads(
    subprocess.check_output(
        [
            "cargo",
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "x86_64-pc-windows-msvc",
        ]
    )
)
rows = []
for p in meta["packages"]:
    if p["source"] is None:
        continue
    package = f"{p['name']}-{p['version']}"
    root = Path(p["manifest_path"]).parent
    paths = [
        f
        for f in root.iterdir()
        if f.is_file()
        and f.name.upper().startswith(
            ("LICENSE", "COPYING", "NOTICE", "LICENCE", "AUTHORS")
        )
    ]
    for f in paths:
        folder = dest / "rust" / package
        folder.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(f, folder / f.name)
    rows.append([package, p["license"], p["repository"]])
for p in md.distributions():
    package = p.metadata["Name"]
    rows.append(
        [
            package,
            p.metadata.get("License-Expression") or p.metadata.get("License"),
            p.version,
        ]
    )
    for item in p.files or []:
        if (
            item.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE"))
            and p.locate_file(item).is_file()
        ):
            folder = dest / "python" / package
            folder.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(
                p.locate_file(item),
                folder / str(item).replace("/", "_").replace("\\", "_"),
            )
(dest / "inventaire.json").write_text(
    json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8"
)
