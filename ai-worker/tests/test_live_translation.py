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

    def test_voice_numbers_follow_returns_and_require_independent_name_evidence(self):
        import numpy as np
        from live_voices import SpeakerMemory

        a, b = np.asarray([1.0, 0.0]), np.asarray([0.0, 1.0])
        memory = SpeakerMemory([dict(id=9, name="Camille", embedding=a)])
        self.assertEqual(memory.assign({0: a, 1: b}), {0: 1, 1: 2})
        self.assertIsNone(memory.suggestion(1))
        self.assertEqual(memory.assign({0: b, 1: a}, fresh={}), {0: 2, 1: 1})
        self.assertIsNone(memory.suggestion(1))
        self.assertEqual(memory.assign({0: a}), {0: 1})
        self.assertEqual(memory.suggestion(1), "Camille")
        self.assertEqual(memory.assign({0: (a + b) / 2**0.5}), {})
        self.assertEqual(len(memory.centres), 2)

    def test_word_attribution_preserves_text_order_marks_overlap_and_uncertainty(self):
        from live_voices import speaker_phrases, OVERLAP, UNKNOWN

        words = [
            (0, 0.3, "Salut"),
            (0.3, 0.9, " !"),
            (1, 1.4, " Oui"),
            (1.4, 2.0, " demain."),
        ]
        turns = [(0, 0.3, 1), (0.3, 0.9, OVERLAP), (1, 1.4, 2)]
        self.assertEqual(
            speaker_phrases(words, turns, "Salut ! Oui demain."),
            [(1, "Salut"), (OVERLAP, "!"), (2, "Oui"), (UNKNOWN, "demain.")],
        )
        self.assertEqual(speaker_phrases([], [], "Bonjour."), [(UNKNOWN, "Bonjour.")])

    def test_final_translation_rechecks_greedy_and_reuses_only_an_exact_full_decode(
        self,
    ):
        from concurrent.futures import Future
        from threading import Event
        from unittest.mock import Mock
        from live import Track, Preview

        track = Track(0, Path("."))
        track.translate = Mock(return_value="Good evening")
        payload = {"track": 0, "start": 20, "text": "Bonsoir", "final": False}
        for full, previous_text, expected_calls in [
            (False, "Bonsoir", 1),
            (True, "Bonjour", 1),
            (True, "Bonsoir", 0),
        ]:
            future = Future()
            future.set_result("Cached")
            track.preview = Preview(previous_text, "fr", full, Event(), future)
            events = []
            track.confirm(
                payload,
                [(None, "Bonsoir")],
                "fr",
                8000,
                {},
                lambda _, live: events.append(live),
            )
            self.assertEqual(track.translate.call_count, expected_calls)
            self.assertEqual(
                events[-1]["translated"], "Good evening" if expected_calls else "Cached"
            )
            self.assertTrue(events[-1]["final"])
            self.assertIsNone(track.preview)
            track.translate.reset_mock()

    def test_forced_cut_retains_unfinished_words_and_untimed_originals(self):
        from types import SimpleNamespace as Item
        from live import RATE, decoded_text

        parts = [
            Item(
                words=[
                    Item(start=0.1, end=1.1, word=" Bon"),
                    Item(start=5.6, end=5.8, word=" soir"),
                ],
                text="Bon soir",
            )
        ]
        text, words, consumed = decoded_text(parts, 6 * RATE, True)
        self.assertEqual((text, consumed), ("Bon", round(RATE * 1.1)))
        self.assertEqual(len(words), 1)
        parts.append(Item(start=5.8, end=6.0, text="encore", words=[]))
        parts[0].start, parts[0].end = 0.1, 5.8
        self.assertEqual(decoded_text(parts, 6 * RATE, True)[0], "Bon soir encore")
        self.assertEqual(decoded_text(parts, 6 * RATE, True)[2], 6 * RATE)
