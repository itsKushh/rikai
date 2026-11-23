import hashlib
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from huggingface_hub.file_download import HfFileMetadata

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from models import install


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

        def head(url, **_):
            name = url.rsplit("/", 1)[1]
            self.assertIn(f"/resolve/{revision}/", url)
            return HfFileMetadata(
                commit_hash=revision,
                etag=hashlib.sha256(content[name]).hexdigest(),
                location=url,
                size=len(content[name]),
                xet_file_data=None,
            )

        transfers = []
        payloads = {**content, "model.bin": b"damaged"}

        def download(url, stream, **_):
            name = url.rsplit("/", 1)[1]
            transfers.append(name)
            stream.write(payloads[name])

        with (
            tempfile.TemporaryDirectory() as cache,
            patch("huggingface_hub.HfApi.model_info", return_value=metadata),
            patch("huggingface_hub.file_download.get_hf_file_metadata", side_effect=head),
            patch("huggingface_hub.file_download.http_get", side_effect=download),
            patch(
                "huggingface_hub.file_download.try_to_load_from_cache",
                side_effect=OSError("WinError 448"),
            ),
        ):
            params = {"preset": "Rapide", "consent": True, "cache": cache}
            with self.assertRaisesRegex(ValueError, "model.bin"):
                install(params, lambda _: None)

            transfers.clear()
            payloads["model.bin"] = content["model.bin"]
            result = install(params, lambda _: None)
            self.assertEqual(transfers, ["model.bin", "tokenizer.json"])
            self.assertEqual(result["repo"], "Systran/faster-whisper-tiny")
            self.assertEqual(result["revision"], revision)
            folder = Path(result["folder"])
            self.assertTrue(folder.is_relative_to(cache))
            self.assertNotIn("snapshots", folder.parts)
            for name, data in content.items():
                path = folder / name
                self.assertTrue(stat.S_ISREG(path.lstat().st_mode))
                self.assertEqual(path.read_bytes(), data)

            transfers.clear()
            self.assertEqual(install(params, lambda _: None), result)
            self.assertEqual(transfers, [])
