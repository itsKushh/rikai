import hashlib
import os
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import updates


class Updates(unittest.TestCase):
    def test_only_newer_stable_verified_assets(self):
        payload = b"installer"
        asset = {
            "id": 45,
            "name": updates.INSTALLER,
            "size": len(payload),
            "state": "uploaded",
            "digest": "sha256:" + hashlib.sha256(payload).hexdigest(),
        }
        release = {
            "tag_name": "v0.10.0",
            "draft": False,
            "prerelease": False,
            "assets": [asset],
            "body": "## Notes",
        }
        reply = MagicMock()
        reply.__enter__.return_value = reply
        reply.json.return_value = release
        with (
            patch.object(updates, "connected", return_value="test"),
            patch.object(updates, "github", return_value=reply),
        ):
            check = lambda v: updates.check_update({"version": v}, lambda *_: None)
            self.assertEqual(check("0.2.0")["state"], "available")
            self.assertEqual(check("0.10.0")["state"], "current")
            release["prerelease"] = True
            self.assertEqual(check("0.2.0")["state"], "current")
            release["prerelease"] = False
            asset["digest"] = None
            with self.assertRaises(ValueError):
                check("0.2.0")

    def test_redirect_integrity_and_no_token_at_asset_host(self):
        data = b"Rikai installer"
        offer = {
            "state": "available",
            "version": "0.2.0",
            "size": len(data),
            "asset": 45,
            "sha256": hashlib.sha256(data).hexdigest(),
        }
        api = MagicMock(
            is_redirect=True,
            headers={"Location": "https://release-assets.githubusercontent.com/test"},
        )
        cdn = MagicMock(status_code=200)
        cdn.__enter__.return_value = cdn
        cdn.iter_content.return_value = [data[:3], data[3:]]
        counts = []
        with (
            TemporaryDirectory() as d,
            patch.dict(os.environ, LOCALAPPDATA=d),
            patch.object(updates, "check_update", return_value=offer),
            patch.object(updates, "connected", return_value="private"),
            patch.object(updates, "github", return_value=api),
            patch.object(updates.requests, "get", return_value=cdn) as fetch,
        ):
            params = {"version": "0.1.0", "target": "0.2.0"}
            out = updates.download_update(
                params, lambda s, received, total: counts.append((received, total))
            )
            self.assertEqual(out["state"], "ready")
            self.assertNotIn("headers", fetch.call_args.kwargs)
            self.assertEqual(counts[-1], (len(data), len(data)))
            exe = next(Path(d).rglob("*.exe"))
            self.assertEqual(exe.read_bytes(), data)
            cdn.iter_content.return_value = [b"X" * len(data)]
            with self.assertRaises(ValueError):
                updates.download_update(params, lambda *a, **k: None)
            self.assertEqual(exe.read_bytes(), data)
            self.assertFalse(list(Path(d).rglob("*.part")))
            api.headers["Location"] = "https://example.org/redirect"
            with self.assertRaises(ValueError):
                updates.download_update(params, lambda *a, **k: None)

    def test_disconnected_check_is_quiet(self):
        with (
            patch.object(updates, "connected", return_value=None),
            patch.object(updates, "github") as server,
        ):
            self.assertEqual(
                updates.check_update({"version": "0.2.0"}, None),
                {"state": "disconnected"},
            )
            server.assert_not_called()

    def test_apply_rechecks_bytes_and_updates_the_running_installation(self):
        with TemporaryDirectory() as d:
            root = Path(d)
            app = root / "custom location"
            (app / "worker").mkdir(parents=True)
            (app / "rikai.exe").write_bytes(b"desktop")
            setup = root / "Rikai" / "updates" / "0.2.0" / updates.INSTALLER
            setup.parent.mkdir(parents=True)
            setup.write_bytes(b"installer")
            params = {
                "target": "0.2.0",
                "version": "0.1.0",
                "sha256": hashlib.sha256(b"installer").hexdigest(),
            }
            with (
                patch.dict(os.environ, LOCALAPPDATA=d),
                patch.object(updates.sys, "platform", "win32"),
                patch.object(
                    updates.sys, "executable", str(app / "worker" / "rikai-worker.exe")
                ),
                patch.object(updates.subprocess, "DETACHED_PROCESS", 8, create=True),
                patch.object(updates.subprocess, "Popen") as launch,
            ):
                for restart in [False, True]:
                    self.assertEqual(
                        updates.apply_update({**params, "restart": restart}, None)[
                            "state"
                        ],
                        "applying",
                    )
                    cmd = launch.call_args.args[0]
                    destinations = [arg[5:] for arg in cmd if arg.startswith("/DIR=")]
                    self.assertEqual(len(destinations), 1)
                    self.assertTrue(app.samefile(destinations[0]))
                    self.assertIn("/RIKAI_RESTART=" + str(int(restart)), cmd)
                launch.reset_mock()
                setup.write_bytes(b"altered")
                with self.assertRaises(ValueError):
                    updates.apply_update({**params, "restart": True}, None)
                launch.assert_not_called()
