"""Installation explicite de modèles publics. Aucune donnée de réunion n'est lue ici."""

import hashlib
import re
from pathlib import Path

PRESETS = {"Rapide": "tiny", "Équilibré": "small", "Haute qualité": "medium"}


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
    from huggingface_hub import HfApi, hf_hub_download

    size = PRESETS[p["preset"]]
    repo = "Systran/faster-whisper-" + size
    metadata = HfApi().model_info(repo, files_metadata=True, token=False)
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
    for entry in files:
        path = folder / entry.rfilename
        if matches_hub_file(path, entry):
            continue
        progress(f"Téléchargement de {entry.rfilename}")
        # local_dir écrit des fichiers ordinaires. force_download empêche le Hub
        # de copier depuis un ancien snapshot dont Windows peut refuser les liens.
        hf_hub_download(
            repo_id=repo,
            filename=entry.rfilename,
            revision=metadata.sha,
            local_dir=folder,
            force_download=True,
            token=False,
        )
        progress(f"Vérification de {entry.rfilename}")
        if not matches_hub_file(path, entry):
            raise ValueError(
                f"Vérification de {entry.rfilename} en échec. Relancez le téléchargement."
            )
    return {"folder": str(folder), "repo": repo, "revision": metadata.sha}


def capabilities(p, progress):
    import ctranslate2
    import platform
    import sys

    return {
        "python": sys.version.split()[0],
        "cpu": platform.machine(),
        "cuda": ctranslate2.get_cuda_device_count(),
    }
