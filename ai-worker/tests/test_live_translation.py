import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
import wave

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from live import read_frames
from translation import translation_input


class LiveTranslation(unittest.TestCase):
    def test_slang_only_changes_the_translation_input(self):
        for source, expected in [
            ("Salut fret !", "Salut mon frère !"),
            ("Le fret arrive demain.", "Le fret arrive demain."),
            ("Merci frérot !", "Merci mon frère !"),
            ("Mon reuf trouve ça chelou.", "Mon frère trouve ça bizarre."),
            ("Je vénère cet artiste.", "Je vénère cet artiste."),
        ]:
            self.assertEqual(
                translation_input(source, "fr").casefold(), expected.casefold()
            )
            self.assertEqual(translation_input(source, "en"), source)

    def test_live_reader_waits_and_resumes_at_the_committed_sample(self):
        with TemporaryDirectory() as folder:
            path = Path(folder) / "piste-1.wav"
            self.assertIsNone(read_frames(path, 0))
            path.write_bytes(b"RIFF" + bytes(4) + b"WAVE" + bytes(8192))
            self.assertIsNone(read_frames(path, 0))
            for count in (3, 8):
                with wave.open(str(path), "wb") as wav:
                    wav.setparams((1, 2, 16000, 0, "NONE", ""))
                    wav.writeframes(
                        b"".join(i.to_bytes(2, "little") for i in range(count))
                    )
                if count == 3:
                    self.assertEqual(read_frames(path, 0).tolist(), [0, 1, 2])
                    self.assertIsNone(read_frames(path, 3))
                else:
                    self.assertEqual(read_frames(path, 3).tolist(), list(range(3, 8)))
