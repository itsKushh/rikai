"""cuBLAS pour le binaire Windows CT2 4.9.1 (CUDA 12.8, chargement dynamique)."""

import ctypes
from typing import NamedTuple
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import zipfile
from transfers import download_large


class Wheel(NamedTuple):
    package: str
    version: str
    size: int
    sha256: str


WHEELS = (
    Wheel(
        "nvidia-cuda-nvrtc-cu12",
        "12.8.93",
        73586838,
        "7a4b6b2904850fe78e0bd179c4b655c404d4bb799ef03ddc60804247099ae909",
    ),
    Wheel(
        "nvidia-cublas-cu12",
        "12.8.5.5",
        567543364,
        "1e272895b82946b4db6f592d9080291fb60f78c9fe253a5c71ba5ebb74864c3e",
    ),
)
RELEASE = "cublas-12.8.5.5-nvrtc-12.8.93"
LIBRARIES = (
    "nvrtc-builtins64_128.dll",
    "nvrtc64_120_0.dll",
    "nvrtc64_120_0.alt.dll",
    "cublasLt64_12.dll",
    "cublas64_12.dll",
)
_dll_directory = None
_handles = {}


def runtime_root():
    return Path(os.environ["LOCALAPPDATA"]) / "Rikai" / "nvidia"


def ready(folder):
    return (folder / "manifest.json").is_file() and all(
        (folder / lib).is_file() for lib in LIBRARIES
    )


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def intact(folder):
    if not ready(folder):
        return False
    try:
        expected = json.loads((folder / "manifest.json").read_text(encoding="utf-8"))
        return all(digest(folder / name) == expected[name] for name in LIBRARIES)
    except (OSError, ValueError, KeyError):
        return False


def extract(archive, folder, package):
    with zipfile.ZipFile(archive) as wheel:
        for info in wheel.infolist():
            path = Path(info.filename)
            if path.name in LIBRARIES and "bin" in path.parts:
                target = folder / path.name
            elif path.name.upper().startswith(("LICENSE", "NOTICE")):
                target = folder / (package + "-" + path.name)
            else:
                continue
            with wheel.open(info) as source, target.open("wb") as out:
                shutil.copyfileobj(source, out)


def fetch(session, wheel, parent, report):
    manifest = session.get(
        f"https://pypi.org/pypi/{wheel.package}/{wheel.version}/json", timeout=20
    )
    manifest.raise_for_status()
    match = next(
        (
            item
            for item in manifest.json()["urls"]
            if item["filename"].endswith("win_amd64.whl")
            and item["digests"]["sha256"] == wheel.sha256
        ),
        None,
    )
    if not match:
        raise ValueError(f"Distribution vérifiée introuvable pour {wheel.package}")
    part = parent / f"{wheel.package}-{wheel.version}.part"
    download_large(session, match["url"], part, wheel.size, report)
    if digest(part) != wheel.sha256:
        part.unlink()
        raise ValueError(f"Intégrité de {wheel.package} en échec")
    return part


def install_runtime(params, progress):
    if sys.platform != "win32" or params.get("consent") is not True:
        raise ValueError(
            "Le téléchargement NVIDIA requiert Windows et une confirmation."
        )
    import requests
    from filelock import FileLock

    total = sum(w.size for w in WHEELS)
    parent = runtime_root()
    parent.mkdir(parents=True, exist_ok=True)
    dest = parent / RELEASE
    previous = parent / (RELEASE + ".previous")
    with FileLock(parent / "installation.lock", timeout=1):
        if not dest.exists() and previous.exists():
            previous.rename(dest)
        progress("Vérification des bibliothèques NVIDIA", received=0, total=total)
        if not intact(dest):
            for entry in parent.glob(".installation-*"):
                if entry.is_dir():
                    shutil.rmtree(entry)
            with tempfile.TemporaryDirectory(
                dir=parent, prefix=".installation-"
            ) as tmp:
                folder = Path(tmp) / "runtime"
                folder.mkdir()
                parts = []
                done = 0
                with requests.Session() as session:
                    for wheel in WHEELS:
                        stage = f"Téléchargement de {wheel.package}"
                        part = fetch(
                            session,
                            wheel,
                            parent,
                            lambda n: progress(stage, received=done + n, total=total),
                        )
                        done += wheel.size
                        progress(
                            f"Extraction de {wheel.package}", received=done, total=total
                        )
                        extract(part, folder, wheel.package)
                        parts.append(part)
                manifest = {name: digest(folder / name) for name in LIBRARIES}
                (folder / "manifest.json").write_text(
                    json.dumps(manifest), encoding="utf-8"
                )
                if dest.exists():
                    if previous.exists():
                        shutil.rmtree(previous)
                    dest.rename(previous)
                try:
                    folder.rename(dest)
                except OSError:
                    if previous.exists():
                        previous.rename(dest)
                    raise
            for part in parts:
                part.unlink()
        if previous.exists():
            shutil.rmtree(previous)
    progress("Bibliothèques NVIDIA vérifiées", received=total, total=total)
    return {"runtime": RELEASE}


def activate():
    global _dll_directory
    if sys.platform != "win32" or _handles:
        return
    folder = runtime_root() / RELEASE
    if not ready(folder):
        return  # Le toolkit système peut également fournir cuBLAS.
    os.environ["PATH"] = str(folder) + os.pathsep + os.environ.get("PATH", "")
    _dll_directory = os.add_dll_directory(str(folder))
    try:
        loaded = {
            name: ctypes.WinDLL(str(folder / name))
            for name in ("nvrtc64_120_0.dll", "cublasLt64_12.dll", "cublas64_12.dll")
        }
    except OSError as exc:
        raise RuntimeError(
            f"Chargement de cuBLAS impossible. Réinstallez le moteur NVIDIA. {exc}"
        ) from exc
    _handles.update(loaded)


def runtime_versions():
    activate()
    if not _handles:
        return {}
    major = ctypes.c_int()
    result = _handles["cublas64_12.dll"].cublasGetProperty(0, ctypes.byref(major))
    if result != 0:
        raise RuntimeError(f"cuBLAS n’a pas fourni sa version ({result})")
    return {"cublas_major": major.value}
