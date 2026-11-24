"""Anonymous updates from the public Rikai release channel."""

import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlsplit

import requests

REPO = "itsKushh/rikai"
API = "https://api.github.com/repos/" + REPO
INSTALLER = "Rikai-Setup-x64.exe"


def version(text):
    match = re.fullmatch(r"v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", str(text))
    return tuple(map(int, match.groups())) if match else None


def github(path, **kwargs):
    headers = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "Rikai-Updater",
    }
    headers.update(kwargs.pop("headers", {}))
    return requests.get(
        API + path,
        headers=headers,
        auth=lambda request: request,
        allow_redirects=False,
        timeout=(10, 35),
        **kwargs,
    )


def check_update(p, _progress):
    with github("/releases/latest") as response:
        if response.status_code == 404:
            with github("/releases?per_page=1") as repo:
                if repo.status_code == 200:
                    return {"state": "current"}
                raise ValueError(
                    "GitHub ne permet pas de lire les versions de ce dépôt."
                )
        response.raise_for_status()
        release = response.json()
    latest, current = version(release["tag_name"]), version(p["version"])
    if (
        not latest
        or not current
        or release["draft"]
        or release["prerelease"]
        or latest <= current
    ):
        return {"state": "current"}
    asset = next(
        (
            a
            for a in release["assets"]
            if a["name"] == INSTALLER and a["state"] == "uploaded"
        ),
        None,
    )
    if (
        not asset
        or not re.fullmatch(r"sha256:[0-9a-f]{64}", asset.get("digest") or "")
        or not 0 < asset["size"] < 2**31
    ):
        raise ValueError("La version GitHub ne contient pas d’installeur vérifiable.")
    notes = (release.get("body") or "")[:16000]
    locale = p.get("locale", "fr")
    if locale in {"en", "es", "ru", "ja", "zh"}:
        tag = release["tag_name"]
        try:
            with github(
                f"/contents/releases/{tag}.i18n.json?ref={tag}",
                headers={"Accept": "application/vnd.github.raw+json"},
            ) as translated:
                if translated.ok and len(translated.content) <= 180_000:
                    candidate = translated.json().get(locale)
                    if isinstance(candidate, str) and candidate.strip():
                        notes = candidate[:16000]
        except (requests.RequestException, ValueError, AttributeError):
            pass  # The published release body is available even without the optional translation.
    return {
        "state": "available",
        "version": release["tag_name"].lstrip("v"),
        "notes": notes,
        "asset": asset["id"],
        "size": asset["size"],
        "sha256": asset["digest"][7:],
    }


def download_update(p, progress):
    # Refresh the authoritative release metadata before selecting the executable.
    release = check_update(p, progress)
    if release.get("state") != "available" or release["version"] != p["target"]:
        raise ValueError("La version proposée a changé. Vérifiez les mises à jour.")
    folder = Path(os.environ["LOCALAPPDATA"]) / "Rikai" / "updates" / release["version"]
    folder.mkdir(parents=True, exist_ok=True)
    dest = folder / INSTALLER
    temp = folder / (INSTALLER + ".part")
    response = github(
        f"/releases/assets/{release['asset']}",
        headers={"Accept": "application/octet-stream"},
        stream=True,
    )
    if response.is_redirect:
        location = response.headers["Location"]
        response.close()
        url = urlsplit(location)
        host = url.hostname or ""
        if url.scheme != "https" or not (
            host.endswith(".githubusercontent.com") or host.endswith(".github.com")
        ):
            raise ValueError("Adresse de téléchargement GitHub inattendue.")
        response = requests.get(
            location,
            stream=True,
            auth=lambda request: request,
            allow_redirects=False,
            timeout=(10, 60),
        )
    with response:
        response.raise_for_status()
        if response.status_code != 200:
            raise ValueError("Téléchargement GitHub incomplet.")
        received, total, checksum = 0, release["size"], hashlib.sha256()
        try:
            with temp.open("wb") as output:
                for chunk in response.iter_content(256 * 1024):
                    received += len(chunk)
                    if received > total:
                        raise ValueError("Taille de l’installeur incorrecte.")
                    output.write(chunk)
                    checksum.update(chunk)
                    progress(
                        "Téléchargement de la mise à jour",
                        received=received,
                        total=total,
                    )
            if received != total or checksum.hexdigest() != release["sha256"]:
                raise ValueError(
                    "L’empreinte de l’installeur ne correspond pas à GitHub."
                )
            temp.replace(dest)
        finally:
            temp.unlink(missing_ok=True)
    return {**release, "state": "ready"}


def apply_update(p, _progress):
    target = p["target"]
    if (
        sys.platform != "win32"
        or not version(target)
        or version(target) <= version(p["version"])
    ):
        raise ValueError("Version de mise à jour invalide.")
    exe = Path(os.environ["LOCALAPPDATA"]) / "Rikai" / "updates" / target / INSTALLER
    with exe.open("rb") as f:
        if hashlib.file_digest(f, "sha256").hexdigest() != p["sha256"]:
            raise ValueError("L’installeur a changé depuis le téléchargement.")
    install_dir = Path(sys.executable).resolve().parent.parent
    if not (install_dir / "rikai.exe").is_file():
        raise ValueError("Dossier d’installation absent")
    subprocess.Popen(
        [
            str(exe),
            "/SP-",
            f"/DIR={install_dir}",
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/NORESTARTAPPLICATIONS",
            "/RIKAI_RESTART=" + ("1" if p["restart"] else "0"),
        ],
        cwd=exe.parent,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
        creationflags=subprocess.DETACHED_PROCESS,
    )
    return {"state": "applying"}
