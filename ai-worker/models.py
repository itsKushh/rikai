"""Installation explicite de modèles publics. Aucune donnée de capture n'est lue ici."""

import json
import hashlib
import os
import re
from dataclasses import dataclass
from package_storage import configuration, locations
from transfers import download_large
from pathlib import Path

PRESETS = {
    "Rapide": "Systran/faster-whisper-base",
    "Équilibré": "Systran/faster-whisper-small",
    "Haute qualité": "Systran/faster-whisper-medium",
    "Précision Turbo": "mobiuslabsgmbh/faster-whisper-large-v3-turbo",
}


# The release pins the weights used by the bundled summarizer and translator.
GGUF = "qwen2.5-3b-instruct-q4_k_m.gguf"
GGUF_CONTEXT = "Qwen_Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
PACKAGES = {
    "Résumé contextualisé": (
        "bartowski/Qwen_Qwen3-4B-Instruct-2507-GGUF",
        "ae44f08e1392f39c0e474af10c3ff8355c8b6688",
        "summary",
        [GGUF_CONTEXT, "README.md"],
    ),
    "Résumé local": (
        "Qwen/Qwen2.5-3B-Instruct-GGUF",
        "7dabda4d13d513e3e842b20f0d435c732f172cbe",
        "summary",
        [GGUF, "LICENSE"],
    ),
    "Traduction locale": (
        "jncraton/m2m100_418M-ct2-int8",
        "7c1b2620a4e58dacecbd8bf89cfd6da7eb9eb7b0",
        "translation",
        [
            "model.bin",
            "config.json",
            "shared_vocabulary.json",
            "sentencepiece.bpe.model",
        ],
    ),
}


@dataclass(frozen=True)
class ModelFile:
    rfilename: str
    size: int
    url: str
    digest: str
    git_blob: bool = False


VOICE_PRESET = "Identification des voix"
VOICE_FILES = (
    ModelFile(
        "segmentation.onnx",
        5992913,
        "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/9403a6902bb58e3d5ae8c7e77c3422de279db2e0/model.onnx",
        "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
    ),
    ModelFile(
        "embedding.onnx",
        29596979,
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx",
        "98daf9bcf1c8abf3db374fec4c27a5eda7c58705bb3f180d057b085a72a88bb5",
    ),
)
VOICE_REVISION = hashlib.sha256(
    "".join(f.digest for f in VOICE_FILES).encode()
).hexdigest()[:40]


def voice_paths(folder):
    return dict(
        path=str(folder),
        segmentation=str(folder / "segmentation.onnx"),
        embedding=str(folder / "embedding.onnx"),
    )


def inventory(cache):
    items = {}
    for marker in Path(cache).glob("*/*/rikai-install.json"):
        try:
            m = json.loads(marker.read_text(encoding="utf8"))
            if all(
                (marker.parent / name).stat().st_size == size
                for name, size in m["files"].items()
            ):
                result = dict(m["result"], folder=str(marker.parent))
                if result.get("kind") == "summary":
                    weights = PACKAGES[m["preset"]][3][0]
                    if weights not in m["files"]:
                        continue
                    result["path"] = str(marker.parent / weights)
                elif result.get("kind") == "voices":
                    if m["files"] != {f.rfilename: f.size for f in VOICE_FILES}:
                        continue
                    result.update(voice_paths(marker.parent))
                elif result.get("kind") == "translation":
                    result["path"] = str(marker.parent)
                items[m["preset"]] = result
        except (OSError, KeyError, TypeError, ValueError):
            continue
    # Versions installed before Rikai 0.2 had no receipt.
    for preset, repo in PRESETS.items():
        if preset in items:
            continue
        for folder in sorted(
            (Path(cache) / repo.split("/")[-1].removeprefix("faster-whisper-")).glob(
                "*"
            )
        ):
            if (folder / "rikai-install.json").exists() or any(folder.glob("*.part")):
                continue
            if re.fullmatch("[a-f0-9]{40}", folder.name) and all(
                (folder / f).is_file()
                for f in ("model.bin", "config.json", "tokenizer.json")
            ):
                items[preset] = {
                    "folder": str(folder),
                    "repo": repo,
                    "revision": folder.name,
                }
                break
    # A completed GGUF can survive a previous run that exited before writing its receipt.
    if "Résumé local" not in items:
        repo, revision, group, _ = PACKAGES["Résumé local"]
        folder = Path(cache) / group / revision
        model = folder / GGUF
        if model.is_file() and model.stat().st_size == 2104932768:
            with model.open("rb") as f:
                if (
                    hashlib.file_digest(f, "sha256").hexdigest()
                    == "626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d"
                ):
                    items["Résumé local"] = dict(
                        repo=repo,
                        revision=revision,
                        kind=group,
                        folder=str(folder),
                        path=str(model),
                    )
    return items


def installed_packages(roots):
    installed = {}
    for root in reversed(roots):
        installed.update(inventory(Path(root) / "modeles"))
    return installed


def matches_hub_file(path, entry):
    if not path.is_file() or path.stat().st_size != entry.size:
        return False
    if not entry.git_blob:
        with path.open("rb") as handle:
            return hashlib.file_digest(handle, "sha256").hexdigest() == entry.digest
    blob = hashlib.sha1(f"blob {entry.size}\0".encode() + path.read_bytes())
    return blob.hexdigest() == entry.digest


def distribution(preset):
    if preset == VOICE_PRESET:
        return "k2-fsa/sherpa-onnx", VOICE_REVISION, "voices", VOICE_FILES
    from huggingface_hub import HfApi, hf_hub_url

    revision = None
    if preset in PACKAGES:
        repo, revision, size, needed = PACKAGES[preset]
        optional = ()
    else:
        repo = PRESETS[preset]
        size = repo.split("/")[1].removeprefix("faster-whisper-")
        needed = ["config.json", "model.bin", "tokenizer.json"]
        optional = ("vocabulary.txt", "vocabulary.json", "preprocessor_config.json")
    metadata = HfApi().model_info(
        repo, revision=revision, files_metadata=True, token=False, timeout=20
    )
    if not re.fullmatch(r"[0-9a-f]{40}", metadata.sha or "") or (
        revision and revision != metadata.sha
    ):
        raise ValueError("Révision du modèle invalide")
    files = [
        ModelFile(
            f.rfilename,
            f.size,
            hf_hub_url(repo, f.rfilename, revision=metadata.sha),
            f.lfs.sha256 if f.lfs else f.blob_id,
            not f.lfs,
        )
        for f in metadata.siblings
        if f.rfilename in needed or f.rfilename in optional
    ]
    if not set(needed).issubset(f.rfilename for f in files):
        raise ValueError("Distribution de modèle incomplète")
    return repo, metadata.sha, size, files


def install(p, progress):
    if p.get("consent") is not True:
        raise ValueError("Autorisation de téléchargement requise")
    from filelock import FileLock
    from huggingface_hub.utils import get_session

    preset = p["preset"]
    progress("Lecture de la liste des fichiers du modèle")
    repo, revision, size, files = distribution(preset)
    cache = Path(p["cache"]) if p.get("cache") else locations()[0] / "modeles"
    folder = cache / size / revision
    folder.mkdir(parents=True, exist_ok=True)
    total = sum(entry.size for entry in files)
    done = 0
    with FileLock(folder / ".installation.lock", timeout=1):
        missing = []
        for entry in files:
            progress(f"Vérification de {entry.rfilename}", received=done, total=total)
            if matches_hub_file(folder / entry.rfilename, entry):
                done += entry.size
            else:
                missing.append(entry)
        for entry in missing:
            stage = f"Téléchargement de {entry.rfilename}"
            progress(stage, received=done, total=total)
            url = entry.url
            part = folder / (entry.rfilename + ".part")
            if entry.size >= 64 * 1024**2:
                download_large(
                    get_session(),
                    url,
                    part,
                    entry.size,
                    lambda n: progress(stage, received=done + n, total=total),
                )
            else:
                with get_session().get(url, stream=True, timeout=(15, 60)) as response:
                    response.raise_for_status()
                    with part.open("wb") as handle:
                        for chunk in response.iter_content(128 * 1024):
                            handle.write(chunk)
                            if handle.tell() > entry.size:
                                raise ValueError(
                                    f"Taille inattendue pour {entry.rfilename}."
                                )
                            progress(stage, received=done + handle.tell(), total=total)
            progress(
                f"Vérification de {entry.rfilename}",
                received=done + entry.size,
                total=total,
            )
            if not matches_hub_file(part, entry):
                part.unlink(missing_ok=True)
                raise ValueError(
                    f"Vérification de {entry.rfilename} en échec. Relancez le téléchargement."
                )
            os.replace(part, folder / entry.rfilename)
            done += entry.size
        progress("Modèle vérifié", received=done, total=total)
        result = {"folder": str(folder), "repo": repo, "revision": revision}
        if size == "voices":
            result.update(kind=size, preset=preset, **voice_paths(folder))
        elif preset in PACKAGES:
            result["kind"] = size
            result["preset"] = preset
            result["path"] = str(
                folder / PACKAGES[preset][3][0] if size == "summary" else folder
            )
        marker = folder / "rikai-install.tmp"
        marker.write_text(
            json.dumps(
                {
                    "preset": preset,
                    "files": {f.rfilename: f.size for f in files},
                    "result": result,
                }
            ),
            encoding="utf8",
        )
        marker.replace(folder / "rikai-install.json")
    return result


def capabilities(p, progress):
    import ctranslate2
    import platform
    import sys

    from nvidia_runtime import runtime_versions

    storage = (
        configuration()
        if os.environ.get("LOCALAPPDATA")
        else {"roots": [], "redirects": {}}
    )
    installed = (
        inventory(p["cache"])
        if p.get("cache")
        else installed_packages(storage["roots"])
    )
    try:
        runtime = runtime_versions()
    except (OSError, RuntimeError):
        runtime = {}
    try:
        cuda = ctranslate2.get_cuda_device_count()
    except (RuntimeError, OSError):
        cuda = 0
    return {
        "storage": storage,
        "runtime": runtime,
        "installed": installed,
        "python": sys.version.split()[0],
        "cpu": platform.machine(),
        "cuda": cuda,
    }
