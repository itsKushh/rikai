import hashlib
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from models import install, inventory


class ModelInstall(unittest.TestCase):
    def test_regular_files_retry_and_reuse_with_inaccessible_snapshot(self):
        content = {
            "config.json": b"{}",
            "model.bin": b"weights",
            "tokenizer.json": b"[]",
        }
        revision = "a" * 40
        entries = []
        for name, data in content.items():
            digest = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
            lfs = None
            if name == "model.bin":
                lfs = SimpleNamespace(sha256=hashlib.sha256(data).hexdigest())
            entries.append(
                SimpleNamespace(rfilename=name, size=len(data), lfs=lfs, blob_id=digest)
            )
        metadata = SimpleNamespace(sha=revision, siblings=entries)

        transfers = []
        updates = []
        payloads = {**content, "model.bin": b"damaged"}

        def download(url, *, stream, timeout):
            self.assertTrue(stream)
            self.assertIn(f"/resolve/{revision}/", url)
            name = url.rsplit("/", 1)[1]
            transfers.append(name)
            data = payloads[name]
            response = MagicMock()
            response.__enter__.return_value = response
            response.iter_content.return_value = (data[:1], data[1:])
            return response

        def progress(stage, *, received=None, total=None):
            if total is not None:
                updates.append((stage, received, total))

        with (
            tempfile.TemporaryDirectory() as cache,
            patch("huggingface_hub.HfApi.model_info", return_value=metadata),
            patch("huggingface_hub.utils.get_session", return_value=SimpleNamespace(get=download)),
            patch(
                "huggingface_hub.file_download.try_to_load_from_cache",
                side_effect=OSError("WinError 448"),
            ),
        ):
            params = {"preset": "Rapide", "consent": True, "cache": cache}
            with self.assertRaisesRegex(ValueError, "model.bin"):
                install(params, progress)

            self.assertFalse(list(Path(cache).rglob("*.part")))
            self.assertFalse(list(Path(cache).rglob("model.bin")))
            transfers.clear()
            updates.clear()
            payloads["model.bin"] = content["model.bin"]
            result = install(params, progress)
            self.assertEqual(transfers, ["model.bin", "tokenizer.json"])
            total = sum(map(len, content.values()))
            self.assertIn(("Téléchargement de model.bin", 3, total), updates)
            self.assertEqual(updates[-1], ("Modèle vérifié", total, total))
            self.assertEqual([u[1] for u in updates], sorted(u[1] for u in updates))
            self.assertTrue(all(u[2] == total for u in updates))
            self.assertEqual(result["repo"], "Systran/faster-whisper-base")
            self.assertEqual(result["revision"], revision)
            folder = Path(result["folder"])
            self.assertTrue(folder.is_relative_to(cache))
            self.assertNotIn("snapshots", folder.parts)
            for name, data in content.items():
                path = folder / name
                self.assertTrue(stat.S_ISREG(path.lstat().st_mode))
                self.assertEqual(path.read_bytes(), data)

            transfers.clear()
            self.assertEqual(install(params, progress), result)
            self.assertEqual(transfers, [])
            self.assertEqual(inventory(cache)["Rapide"]["revision"], revision)
            (folder/"model.bin").unlink()
            self.assertNotIn("Rapide",inventory(cache))
