"""Installation explicite de modèles publics. Aucune donnée de réunion n'est lue ici."""

import hashlib
from pathlib import Path

PRESETS = {"Rapide": "tiny", "Équilibré": "small", "Haute qualité": "medium"}


def install(p, progress):
    if p.get("consent") is not True:
        raise ValueError("Autorisation de téléchargement requise")
    from huggingface_hub import HfApi, snapshot_download

    repo = "Systran/faster-whisper-" + PRESETS[p["preset"]]
    metadata = HfApi().model_info(repo, files_metadata=True, token=False)
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
    progress("Téléchargement des poids de transcription")
    folder = Path(
        snapshot_download(
            repo,
            revision=metadata.sha,
            cache_dir=p["cache"],
            token=False,
            allow_patterns=[f.rfilename for f in files],
        )
    )
    progress("Vérification des fichiers")
    for f in files:
        path = folder / f.rfilename
        if path.stat().st_size != f.size:
            raise ValueError("Taille du modèle inattendue")
        if f.lfs:
            with path.open("rb") as handle:
                if hashlib.file_digest(handle, "sha256").hexdigest() != f.lfs.sha256:
                    raise ValueError("Empreinte du modèle incorrecte")
        else:
            h = hashlib.sha1(
                f"blob {f.size}\0".encode() + path.read_bytes()
            ).hexdigest()
            if h != f.blob_id:
                raise ValueError("Fichier de configuration du modèle incorrect")
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
