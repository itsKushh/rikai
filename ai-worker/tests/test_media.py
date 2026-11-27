from pathlib import Path
import tempfile
import unittest
import wave

import av
import numpy as np

from media import import_media, probe


class MediaImport(unittest.TestCase):
    def test_video_preview_and_streamed_audio(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            source = root / "clip.mkv"
            with av.open(str(source), "w") as media:
                video = media.add_stream("ffv1", rate=1)
                video.width, video.height, video.pix_fmt = 96, 64, "yuv420p"
                audio = media.add_stream("pcm_s16le", rate=48000)
                audio.layout = "mono"
                image = av.VideoFrame.from_ndarray(np.zeros((64, 96, 3), dtype=np.uint8), format="rgb24")
                samples = av.AudioFrame.from_ndarray(np.full((1, 48000), 1000, dtype=np.int16), format="s16", layout="mono")
                samples.sample_rate = 48000
                for stream, frame in ((video, image), (audio, samples)):
                    for packet in [*stream.encode(frame), *stream.encode(None)]:
                        media.mux(packet)
            thumb = root / "preview.png"
            before = source.read_bytes()
            info = probe({"path": source, "thumbnail": thumb}, lambda *_: None)
            self.assertTrue(info["video"])
            self.assertEqual((info["width"], info["height"]), (96, 64))
            self.assertTrue(thumb.read_bytes().startswith(b"\x89PNG"))
            result = import_media({"path": source, "directory": root}, lambda *_: None)
            self.assertEqual(result["duration_ms"], 1000)
            with wave.open(str(root / result["file"])) as wav:
                self.assertEqual((wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()), (1, 2, 16000, 16000))
            self.assertEqual(before, source.read_bytes())
            self.assertTrue((root / "finalise").exists())
