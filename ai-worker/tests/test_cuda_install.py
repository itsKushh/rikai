import hashlib
from pathlib import Path
import shutil
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import nvidia_runtime as cuda
from transfers import download_large


class CudaInstall(unittest.TestCase):
    def test_resume_at_existing_byte_with_exact_content_range(self):
        payload = b"0123456789"
        headers_seen = []

        def fetch(url, *, headers, stream, timeout):
            bounds = headers["Range"].removeprefix("bytes=")
            start, end = map(int, bounds.split("-"))
            headers_seen.append(start)
            response = MagicMock()
            response.__enter__.return_value = response
            response.status_code = 206
            response.headers = {"Content-Range": f"bytes {bounds}/10"}
            response.iter_content.return_value = [payload[start : end + 1]]
            return response

        session = SimpleNamespace(get=fetch)
        with tempfile.TemporaryDirectory() as root:
            part = Path(root) / "part"
            part.write_bytes(payload[:3])
            download_large(
                session,
                "https://example.org/poids",
                part,
                len(payload),
                lambda _: None,
                chunk_size=4,
            )
            self.assertEqual(part.read_bytes(), payload)
            self.assertEqual(headers_seen, [3, 7])

    def test_verified_install_reuse_and_repair(self):
        with tempfile.TemporaryDirectory() as tmp:
            parent = Path(tmp)
            src = parent / "fixture.whl"
            with zipfile.ZipFile(src, "w") as z:
                for name in cuda.LIBRARIES:
                    z.writestr("nvidia/cublas/bin/" + name, b"sample " + name.encode())
                z.writestr("package.dist-info/License.txt", "licence")
            expected = hashlib.sha256(src.read_bytes()).hexdigest()
            session = MagicMock()
            session.__enter__.return_value = session
            session.get.return_value.json.return_value = {
                "urls": [
                    {
                        "filename": "test-win_amd64.whl",
                        "digests": {"sha256": expected},
                        "url": "https://example.org/fixture",
                    }
                ]
            }

            def download(_, url, part, size, cb):
                shutil.copyfile(src, part)
                cb(size)

            with (
                patch.object(cuda, "sys", SimpleNamespace(platform="win32")),
                patch.object(cuda, "runtime_root", return_value=parent),
                patch.object(
                    cuda,
                    "WHEELS",
                    [cuda.Wheel("fixture", "1", src.stat().st_size, expected)],
                ),
                patch("requests.Session", return_value=session),
                patch.object(cuda, "download_large", side_effect=download) as get,
            ):
                for mutation in (False, False, True):
                    if mutation:
                        (parent / cuda.RELEASE / cuda.LIBRARIES[0]).write_bytes(
                            b"broken"
                        )
                    res = cuda.install_runtime({"consent": True}, lambda *a, **k: None)
                    self.assertEqual(res["runtime"], cuda.RELEASE)
                    self.assertTrue(cuda.intact(parent / cuda.RELEASE))
                self.assertEqual(get.call_count, 2)
                with self.assertRaises(ValueError):
                    cuda.install_runtime({}, lambda *a, **k: None)
