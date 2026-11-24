import json
import os
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import models
import nvidia_runtime as cuda
import package_storage as storage


class PackageStorage(unittest.TestCase):
    def setUp(self):
        temp = TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.parent = Path(temp.name).resolve()
        env = patch.dict(os.environ, LOCALAPPDATA=str(self.parent))
        env.start()
        self.addCleanup(env.stop)
        self.home = storage.profile()
        self.target = self.parent / "external drive"
        self.target.mkdir()
        self.revision = "e" * 40
        self.package = Path("modeles", "summary", self.revision)
        original = self.home / self.package
        original.mkdir(parents=True)
        (original / models.GGUF).write_bytes(b"test weights")
        # Pre-existing receipts contain absolute paths that can be stale.
        (original / "rikai-install.json").write_text(
            json.dumps(
                {
                    "preset": "Résumé local",
                    "files": {models.GGUF: 12},
                    "result": {
                        "kind": "summary",
                        "path": "stale",
                        "folder": "stale",
                        "revision": self.revision,
                    },
                }
            )
        )
        self.runtime = Path("nvidia", cuda.RELEASE)
        (self.home / self.runtime).mkdir(parents=True)
        for name in (*cuda.LIBRARIES, "manifest.json"):
            (self.home / self.runtime / name).write_bytes(b"runtime")
        (self.home / "reunions.sqlite3").write_bytes(b"meetings")

    def relocate(self, transfer, report=lambda *a, **k: None):
        return storage.change_location(
            {"destination": str(self.target), "transfer": transfer}, report
        )

    def test_keep_then_transfer_across_roots_and_reload_inventory(self):
        old = self.home / self.package / models.GGUF
        new = self.target / self.package / models.GGUF
        keep = self.relocate(False)
        self.assertEqual(Path(keep["installed"]["Résumé local"]["path"]), old)
        self.assertFalse(new.exists())
        self.assertEqual(cuda.runtime_root(), self.target / "nvidia")
        self.assertIn(self.home, storage.locations())
        self.assertTrue(cuda.ready(self.home / self.runtime))
        counts = []
        moved = self.relocate(
            True,
            lambda _, **fields: (
                counts.append((fields["received"], fields["total"])) if fields else None
            ),
        )
        self.assertEqual(counts[-1][0], counts[-1][1])
        self.assertEqual(new.read_bytes(), b"test weights")
        self.assertFalse(old.exists())
        self.assertEqual(Path(moved["installed"]["Résumé local"]["path"]), new)
        self.assertEqual(
            models.installed_packages(storage.locations()), moved["installed"]
        )
        self.assertEqual(
            moved["storage"]["redirects"][str(self.home)], str(self.target)
        )
        self.assertTrue(cuda.ready(self.target / self.runtime))
        self.assertFalse(cuda.ready(self.home / self.runtime))
        self.assertEqual((self.home / "reunions.sqlite3").read_bytes(), b"meetings")
        self.assertEqual(self.relocate(True)["installed"], moved["installed"])

    def test_conflict_and_interruption_preserve_source_and_preference(self):
        dest = self.target / self.package / models.GGUF
        dest.parent.mkdir(parents=True)
        dest.write_bytes(b"other")
        with self.assertRaises(ValueError):
            self.relocate(True)
        self.assertEqual(dest.read_bytes(), b"other")
        self.assertEqual(storage.locations()[0], self.home)
        dest.unlink()

        def interrupt(*args, **kwargs):
            if kwargs:
                raise IOError("unplugged")

        with self.assertRaises(IOError):
            self.relocate(True, interrupt)
        self.assertTrue((self.home / self.package / models.GGUF).exists())
        self.assertEqual(storage.locations()[0], self.home)
        self.assertFalse(list(self.target.rglob(".rikai-*")))
        self.relocate(True)
        self.assertEqual(dest.read_bytes(), b"test weights")

    def test_foreign_files_and_nested_location(self):
        extra = self.home / self.package / "mes-notes.txt"
        extra.write_text("private")
        with self.assertRaises(ValueError):
            storage.change_location(
                {"destination": str(extra.parent), "transfer": True},
                lambda *a, **k: None,
            )
        self.relocate(True)
        self.assertTrue(extra.is_file())
        self.assertFalse((self.target / self.package / extra.name).exists())
