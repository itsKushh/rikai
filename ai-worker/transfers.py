"""Transferts HTTP reprenables pour les poids et les bibliothèques NVIDIA."""

import time


def download_large(session, url, part, size, report, chunk_size=32 * 1024**2):
    from requests import RequestException

    offset = part.stat().st_size if part.is_file() else 0
    if offset > size:
        part.unlink()
        offset = 0
    errors = 0
    last = 0.0
    while offset < size:
        end = min(size, offset + chunk_size) - 1
        try:
            with session.get(
                url + f"?download=true&chunk={offset}",
                headers={"Range": f"bytes={offset}-{end}"},
                stream=True,
                timeout=(15, 60),
            ) as response:
                response.raise_for_status()
                if response.status_code == 206:
                    if (
                        response.headers.get("Content-Range")
                        != f"bytes {offset}-{end}/{size}"
                    ):
                        raise ValueError("Plage du téléchargement incohérente")
                elif response.status_code == 200:
                    offset, end = 0, size - 1
                else:
                    raise ValueError("Réponse inattendue pour le téléchargement")
                with part.open("ab" if offset else "wb") as handle:
                    for chunk in response.iter_content(256 * 1024):
                        if offset + len(chunk) > end + 1:
                            raise ValueError("Plage de téléchargement trop longue")
                        handle.write(chunk)
                        offset += len(chunk)
                        now = time.monotonic()
                        if now - last >= 0.15 or offset == size:
                            report(offset)
                            last = now
                if offset != end + 1:
                    raise IOError("Connexion interrompue avant la fin du bloc")
            errors = 0
        except (RequestException, OSError):
            errors += 1
            if errors >= 3:
                raise
            time.sleep(errors)
