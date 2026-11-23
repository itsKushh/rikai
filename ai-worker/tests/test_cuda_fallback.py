import json
import tempfile
import unittest
import wave
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import processing


class CudaFallback(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        tracks = []
        for name, tones in (("piste-1.wav", [1, 2]), ("piste-2.wav", [3])):
            with wave.open(str(self.root / name), "wb") as writer:
                writer.setparams((1, 2, 16000, 0, "NONE", ""))
                for tone in tones:
                    writer.writeframes(tone.to_bytes(2, "little") * 16000)
            tracks.append({"file": name, "input": {"source": {"kind": "Microphone"}}})
        (self.root / "sources.json").write_text(json.dumps({"tracks": tracks}))

    def run_transcription(self, factory, device="cuda"):
        params = {
            "directory": str(self.root),
            "settings": {"whisper": str(self.root), "device": device, "language": "fr"},
        }
        blocks = processing.blocks
        self.progress = []
        with (
            patch("faster_whisper.WhisperModel", side_effect=factory),
            patch("processing.blocks", side_effect=lambda path: blocks(path, seconds=1)),
        ):
            return processing.transcribe(
                params, lambda stage, **_: self.progress.append(stage)
            )

    def test_late_missing_dll_replays_only_unfinished_block(self):
        calls = []
        devices = []

        def factory(path, *, device, compute_type, local_files_only):
            self.assertTrue(local_files_only)
            devices.append((device, compute_type))

            def decode(data, **options):
                block = round(float(data[0]) * 32768)
                calls.append((device, block))
                yield SimpleNamespace(
                    start=0, end=0.9, words=[], text=f"{device} {block}"
                )
                if device == "cuda" and block == 2:
                    raise RuntimeError(
                        "Library cublas64_12.dll is not found or cannot be loaded"
                    )

            return SimpleNamespace(transcribe=lambda data, **kw: (decode(data, **kw), None))

        result = self.run_transcription(factory)
        self.assertEqual(devices, [("cuda", "float16"), ("cpu", "int8")])
        self.assertEqual(calls, [("cuda", 1), ("cuda", 2), ("cpu", 2), ("cpu", 3)])
        self.assertEqual(result["device"], "cpu")
        self.assertEqual(
            [(s["text"], s["track"], s["start_ms"]) for s in result["segments"]],
            [("cuda 1", "piste-1.wav", 0), ("cpu 3", "piste-2.wav", 0),
             ("cpu 2", "piste-1.wav", 1000)],
        )
        self.assertTrue(any("CUDA indisponible" in stage for stage in self.progress))

    def test_fallback_at_model_load_but_keep_working_cuda(self):
        engine = Mock()
        engine.transcribe.return_value = ([], None)
        for fail in (False, True):
            with self.subTest(cuda_failure=fail):
                devices = []
                def factory(path, *, device, **kw):
                    devices.append(device)
                    if fail and device == "cuda":
                        raise RuntimeError("CUDA failed with error no CUDA-capable device")
                    return engine

                result = self.run_transcription(factory)
                self.assertEqual(devices, ["cuda", "cpu"] if fail else ["cuda"])
                self.assertEqual(result["device"], devices[-1])

    def test_model_errors_and_cpu_errors_are_reported_without_retry(self):
        for device, error in (
            ("cuda", ValueError("Invalid vocabulary")),
            ("cpu", RuntimeError("cublas64_12.dll")),
        ):
            with self.subTest(device=device):
                factory = Mock(side_effect=error)
                with self.assertRaises(type(error)) as raised:
                    self.run_transcription(factory, device)
                self.assertIs(raised.exception, error)
                factory.assert_called_once()

    def test_strict_gpu_never_reports_cpu_success(self):
        params = {"directory": str(self.root), "settings": {"whisper": str(self.root), "device": "cuda", "strict_cuda": True}}
        with patch("faster_whisper.WhisperModel", side_effect=RuntimeError("Library cublas64_12.dll is not found")) as load:
            with self.assertRaisesRegex(RuntimeError, "cublas64_12"):
                processing.transcribe(params, lambda *args, **kwargs: None)
            self.assertEqual(load.call_count, 1)
            self.assertEqual(load.call_args.kwargs["device"], "cuda")
