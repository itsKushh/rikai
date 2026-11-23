from pathlib import Path
from types import SimpleNamespace
import json
import threading
from queue import Empty, Queue
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from processing import align  # noqa: E402
from voices import candidate, enroll  # noqa: E402
from models import install  # noqa: E402


def worker_exchange(request, *, command=None, cancel_on_progress=False, on_progress=None, timeout=25):
    """Conserver stdin ouvert pendant le travail, borner la réponse ET la sortie."""
    process = subprocess.Popen(
        command or [sys.executable, ROOT / "worker.py"],
        cwd=ROOT,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        encoding="utf-8",
    )
    events = Queue()

    def read_events():
        with process.stdout as output:
            for line in output:
                events.put(line)
        events.put(None)

    threading.Thread(target=read_events, daemon=True).start()
    method = request["method"]
    try:
        process.stdin.write(json.dumps(request) + "\n")
        process.stdin.flush()
        while True:
            try:
                line = events.get(timeout=timeout)
            except Empty:
                raise AssertionError(
                    f"{method} : aucune réponse IPC en {timeout} s"
                ) from None
            if line is None:
                raise AssertionError(f"{method} : canal fermé avant la réponse")
            event = json.loads(line)
            if event["event"] in ("result", "error"):
                break
            if on_progress and event["event"] == "progress":
                on_progress(event)
            if cancel_on_progress and event["event"] == "progress":
                process.stdin.write(
                    json.dumps({"id": request["id"], "method": "cancel"}) + "\n"
                )
                process.stdin.flush()
                cancel_on_progress = False
        process.stdin.close()
        return event, process.wait(timeout=timeout)
    finally:
        if not process.stdin.closed:
            process.stdin.close()
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)


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
            with self.subTest(method=method):
                last, code = worker_exchange(
                    dict(version=1, id=17, method=method, params=params)
                )
                self.assertEqual((last["id"], last["event"]), (17, event))
                self.assertEqual(code, 0 if event == "result" else 1)
                if method == "transcribe":
                    self.assertEqual(last["code"], "ValueError")
                    self.assertIn("introuvable", last["message"])

    def test_cancel_inference_with_control_pipe_open(self):
        # Remplacer uniquement le moteur dans ce processus de test. Le vrai
        # protocole attend un ordre d'annulation après le démarrage du travail.
        engine = (
            "import os, threading, worker\n"
            "def wait_for_cancel(params, progress):\n"
            "    progress('prêt')\n"
            "    threading.Event().wait()\n"
            "worker.transcribe = wait_for_cancel\n"
            "os._exit(worker.main())"
        )
        req = {"version": 1, "id": 2, "method": "transcribe", "params": {}}
        line, code = worker_exchange(
            req,
            command=[sys.executable, "-c", engine],
            cancel_on_progress=True,
            timeout=10,
        )
        self.assertEqual((line["code"], code), ("annule", 2))

    def test_silent_worker_has_a_response_deadline(self):
        command = [sys.executable, "-c", "import sys; sys.stdin.read()"]
        with self.assertRaisesRegex(AssertionError, "ping : aucune réponse IPC"):
            worker_exchange(
                {"version": 1, "id": 3, "method": "ping"}, command=command, timeout=0.5
            )

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
