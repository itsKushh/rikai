"""Installation explicite de modèles publics. Aucune donnée de réunion n'est lue ici."""

import hashlib
import os
import re
from transfers import download_large
from pathlib import Path

PRESETS = {
    "Rapide": "Systran/faster-whisper-base",
    "Équilibré": "Systran/faster-whisper-small",
    "Haute qualité": "Systran/faster-whisper-medium",
    "Précision Turbo": "mobiuslabsgmbh/faster-whisper-large-v3-turbo",
}


def matches_hub_file(path, entry):
    if not path.is_file() or path.stat().st_size != entry.size:
        return False
    if entry.lfs:
        with path.open("rb") as handle:
            return hashlib.file_digest(handle, "sha256").hexdigest() == entry.lfs.sha256
    blob = hashlib.sha1(f"blob {entry.size}\0".encode() + path.read_bytes())
    return blob.hexdigest() == entry.blob_id


def install(p, progress):
    if p.get("consent") is not True:
        raise ValueError("Autorisation de téléchargement requise")
    from filelock import FileLock
    from huggingface_hub import HfApi, hf_hub_url
    from huggingface_hub.utils import get_session

    repo = PRESETS[p["preset"]]
    size = repo.split("/")[1].removeprefix("faster-whisper-")
    progress("Lecture de la liste des fichiers du modèle")
    metadata = HfApi().model_info(repo, files_metadata=True, token=False, timeout=20)
    if not re.fullmatch(r"[0-9a-f]{40}", metadata.sha or ""):
        raise ValueError("Révision du modèle invalide")
    needed = ["config.json", "model.bin", "tokenizer.json"]
    files = [
        f
        for f in metadata.siblings
        if f.rfilename in needed
        or f.rfilename
        in ("vocabulary.txt", "vocabulary.json", "preprocessor_config.json")
    ]
    if not set(needed).issubset(f.rfilename for f in files):
        raise ValueError("Distribution de modèle incomplète")
    folder = Path(p["cache"]) / size / metadata.sha
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
            url = hf_hub_url(repo, entry.rfilename, revision=metadata.sha)
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
    return {"folder": str(folder), "repo": repo, "revision": metadata.sha}


def capabilities(p, progress):
    import ctranslate2
    import platform
    import sys

    from nvidia_runtime import runtime_versions

    return {
        "runtime": runtime_versions(),
        "python": sys.version.split()[0],
        "cpu": platform.machine(),
        "cuda": ctranslate2.get_cuda_device_count(),
    }
