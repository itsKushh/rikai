"""Locations of Rikai-managed downloads, independent of the recording library."""

import hashlib
import json
import os
from pathlib import Path
import re
import tempfile

from filelock import FileLock


def profile():
    return Path(os.environ["LOCALAPPDATA"]) / "Rikai"


def configuration():
    home = profile()
    try:
        return json.loads((home / "package-storage.json").read_text(encoding="utf8"))
    except FileNotFoundError:
        return {"roots": [str(home)], "redirects": {}}


def storage_lock():
    home = profile()
    home.mkdir(parents=True, exist_ok=True)
    return FileLock(home / "package-storage.lock", timeout=1)


def locations():
    return [Path(root) for root in configuration()["roots"]]


def _managed_files(root):
    from models import GGUF, PACKAGES, PRESETS
    from nvidia_runtime import LIBRARIES, RELEASE, WHEELS

    names = {
        "model.bin",
        "config.json",
        "tokenizer.json",
        "vocabulary.txt",
        "vocabulary.json",
        "preprocessor_config.json",
        GGUF,
        "LICENSE",
        "shared_vocabulary.json",
        "sentencepiece.bpe.model",
        "rikai-install.json",
    }
    groups = {
        "tiny",
        *(
            repo.split("/")[-1].removeprefix("faster-whisper-")
            for repo in PRESETS.values()
        ),
        *(entry[2] for entry in PACKAGES.values()),
    }
    for group in sorted(groups):
        base = root / "modeles" / group
        _check_path(root, base)
        if not base.is_dir():
            continue
        for revision in sorted(base.iterdir()):
            if not re.fullmatch(r"[0-9a-f]{40}", revision.name):
                continue
            _check_path(root, revision)
            if revision.is_dir():
                managed = names.union(
                    name
                    for _, sha, package_group, files in PACKAGES.values()
                    if group == package_group and sha == revision.name
                    for name in files
                )
                for file in sorted(revision.iterdir()):
                    if file.name.removesuffix(".part") in managed:
                        _check_path(root, file)
                        if file.is_file():
                            yield file
    nvidia = root / "nvidia"
    runtime = nvidia / RELEASE
    _check_path(root, runtime)
    if runtime.is_dir():
        for file in sorted(runtime.iterdir()):
            if file.name in (*LIBRARIES, "manifest.json") or any(
                file.name.startswith(w.package + prefix)
                for w in WHEELS
                for prefix in ("-LICENSE", "-NOTICE")
            ):
                _check_path(root, file)
                if file.is_file():
                    yield file
    for wheel in WHEELS:
        file = nvidia / f"{wheel.package}-{wheel.version}.part"
        _check_path(root, file)
        if file.is_file():
            yield file


def _check_path(root, path):
    # Never follow junctions or symlinks below a managed root when moving files.
    if path.resolve() != root / path.relative_to(root):
        raise ValueError(f"Lien de stockage inattendu : {path}")


def fingerprint(file):
    with file.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").digest()


def change_location(params, progress):
    target = Path(params["destination"])
    if not target.is_absolute() or not target.is_dir():
        raise ValueError("Sélectionnez un dossier existant.")
    target = target.resolve()
    home = profile()
    progress("Emplacement des composants")
    with storage_lock():
        old = configuration()
        roots = [Path(path).resolve() for path in old["roots"]]
        others = [root for root in roots if root != target]
        for root in others:
            for group in ("modeles", "nvidia"):
                if target.is_relative_to(root / group) or root.is_relative_to(
                    target / group
                ):
                    raise ValueError(
                        "Choisissez un dossier en dehors des composants existants."
                    )
        for group in ("modeles", "nvidia"):
            _check_path(target, target / group)
        with tempfile.TemporaryFile(dir=target):
            pass  # Check write access before changing the preference.
        move = params.get("transfer") is True
        redirects = dict(old["redirects"])
        files = (
            [
                (src, target / src.relative_to(root))
                for root in others
                for src in _managed_files(root)
            ]
            if move
            else []
        )
        total = sum(src.stat().st_size for src, _ in files)
        received = 0
        copied = []
        for src, dst in files:
            before = src.stat()
            _check_path(target, dst)
            dst.parent.mkdir(parents=True, exist_ok=True)
            if dst.exists():
                if (
                    not dst.is_file()
                    or dst.stat().st_size != src.stat().st_size
                    or fingerprint(dst) != fingerprint(src)
                ):
                    raise ValueError(f"Un fichier différent existe déjà : {dst}")
                received += src.stat().st_size
            else:
                # A failed copy never exposes a partial package as complete.
                with tempfile.NamedTemporaryFile(
                    dir=dst.parent, prefix=".rikai-", delete=False
                ) as output:
                    tmp = Path(output.name)
                try:
                    checksum = hashlib.sha256()
                    with src.open("rb") as inp, tmp.open("wb") as out:
                        while chunk := inp.read(4 * 1024 * 1024):
                            out.write(chunk)
                            checksum.update(chunk)
                            received += len(chunk)
                            progress(
                                "Transfert des composants",
                                received=received,
                                total=total,
                            )
                        out.flush()
                        os.fsync(out.fileno())
                    if fingerprint(tmp) != checksum.digest():
                        raise IOError("Échec de la vérification de la copie.")
                    if os.name == "nt":
                        tmp.rename(dst)  # Windows rejects an existing destination.
                    else:
                        os.link(tmp, dst)
                finally:
                    tmp.unlink(missing_ok=True)
            progress("Transfert des composants", received=received, total=total)
            copied.append((src, before.st_size, before.st_mtime_ns))
        if move:
            for key, value in list(redirects.items()):
                if Path(value) in others:
                    redirects[key] = str(target)
            redirects.update({str(root): str(target) for root in others})
            redirects.pop(str(target), None)
        # Retain historic roots for offline drives, untransferred components and retries.
        state = {"roots": [str(target), *map(str, others)], "redirects": redirects}
        temp = home / "package-storage.tmp"
        with temp.open("w", encoding="utf8") as output:
            json.dump(state, output, ensure_ascii=False)
            output.flush()
            os.fsync(output.fileno())
        temp.replace(home / "package-storage.json")
        # Only retire verified copies after the new root is durable. A retry can
        # finish cleanup after interruption or when a Windows file lock is released.
        pending = False
        for src, length, modified in copied:
            try:
                now = src.stat()
                if now.st_size != length or now.st_mtime_ns != modified:
                    pending = True
                    continue
                src.unlink()
            except OSError:
                pending = True
        for folder in {src.parent for src, _ in files}:
            for candidate in (folder, folder.parent):
                if candidate in roots:
                    continue
                try:
                    candidate.rmdir()
                except OSError:
                    pass
        from models import installed_packages

        return {
            "storage": {**state, "cleanup_pending": pending},
            "installed": installed_packages(state["roots"]),
        }
