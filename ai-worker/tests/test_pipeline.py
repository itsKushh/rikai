from pathlib import Path
from types import SimpleNamespace
import json
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from processing import align  # noqa: E402
from voices import candidate, enroll  # noqa: E402
from models import install  # noqa: E402


class Timing(unittest.TestCase):
    def test_change_inside_phrase_and_simultaneous_speech(self):
        turns = [
            dict(start_ms=0, end_ms=300, label="A"),
            dict(start_ms=300, end_ms=700, label="B"),
            dict(start_ms=500, end_ms=600, label="C"),
        ]
        words = [
            SimpleNamespace(start=a, end=b, word=t, probability=0.92)
            for a, b, t in [(0, 0.2, "bon"), (0.3, 0.4, " jour"), (0.5, 0.6, " !")]
        ]
        seg = SimpleNamespace(words=words)
        rows = list(align(seg, "app.wav", None, turns, 0))
        self.assertEqual(
            [r["speaker"] for r in rows], ["A", "B", "Chevauchement · B / C"]
        )
        self.assertEqual([r["start_ms"] for r in rows], [0, 300, 500])
        mine = list(align(seg, "mic.wav", "Moi", turns, 1000))
        self.assertEqual(
            [(r["speaker"], r["start_ms"], r["end_ms"]) for r in mine],
            [("Moi", 1000, 1600)],
        )

    def test_near_tie_and_unknown_abstain(self):
        models = [
            {"id": 1, "embedding": [1, 0]},
            {"id": 2, "embedding": [0.95, 0.3122]},
        ]
        self.assertIsNone(candidate([1.0, 0.0], models))
        self.assertIsNone(candidate([-1.0, 0.0], models))
        self.assertEqual(candidate([1.0, 0.0], models[:1])["profile"], 1)

    def test_no_implicit_enrolment_or_download(self):
        for f in (enroll, install):
            with self.assertRaises(ValueError):
                f({}, lambda _: None)

    def test_ipc_and_missing_model(self):
        for method, params, event in [
            ("ping", {}, "result"),
            ("inconnu", {}, "error"),
            (
                "transcribe",
                {"directory": str(ROOT), "settings": {"whisper": str(ROOT / "absent")}},
                "error",
            ),
        ]:
            req = (
                json.dumps(dict(version=1, id=17, method=method, params=params)) + "\n"
            )
            with subprocess.Popen(
                [sys.executable, ROOT / "worker.py"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
            ) as p:
                p.stdin.write(req)
                p.stdin.flush()
                last = json.loads(p.stdout.readline())
                p.wait(timeout=25)
            self.assertEqual((last["id"], last["event"]), (17, event))
            self.assertEqual(p.returncode == 0, event == "result")

    def test_cancel_inference_with_control_pipe_open(self):
        req = {"version": 1, "id": 2, "method": "transcribe", "params": {}}
        with subprocess.Popen(
            [sys.executable, ROOT / "worker.py"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        ) as p:
            p.stdin.write(
                json.dumps(req)
                + "\n"
                + json.dumps({"id": 2, "method": "cancel"})
                + "\n"
            )
            p.stdin.flush()
            line = json.loads(p.stdout.readline())
            self.assertEqual(line["code"], "annule")
            self.assertEqual(p.wait(timeout=10), 2)

    def test_summary_rejects_unknown_evidence(self):
        from processing import generate_report

        class InvalidEvidence:
            def tokenize(self, b):
                return b

            def create_chat_completion(self, **kwargs):
                fact = {"type": "Décision", "texte": "Déployer", "preuves": [999]}
                return {
                    "choices": [
                        {
                            "finish_reason": "stop",
                            "message": {"content": json.dumps({"faits": [fact]})},
                        }
                    ]
                }

        with self.assertRaisesRegex(ValueError, "Preuve"):
            generate_report(
                [dict(id=1, speaker="Moi", text="On hésite.")],
                InvalidEvidence(),
                lambda _: None,
            )

    def test_echo_requires_acoustic_similarity(self):
        from echo import correlation
        import numpy as np

        rng = np.random.default_rng(4)
        voice = rng.normal(size=8000)
        self.assertGreater(
            correlation(voice, np.concatenate([np.zeros(20), voice * 0.15])), 0.99
        )
        self.assertLess(correlation(voice, rng.normal(size=8000)), 0.2)

    def test_synthetic_silence_with_real_vad(self):
        from faster_whisper.vad import get_speech_timestamps
        import numpy as np

        self.assertEqual(get_speech_timestamps(np.zeros(32000, dtype=np.float32)), [])


if __name__ == "__main__":
    unittest.main()
