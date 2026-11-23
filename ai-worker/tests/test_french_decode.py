import sys
import unittest
from pathlib import Path
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from processing import decode_block


class FrenchPassages(unittest.TestCase):
    def test_targeted_retry_preserves_adjacent_segments_and_word_times(self):
        from faster_whisper.transcribe import Segment, Word
        from tokenizers import Tokenizer
        from tokenizers.models import WordLevel
        import numpy as np

        def segment(a, b, text):
            return Segment(
                0, 0, a, b, text, [], -0.1, 1.0, 0.0, [Word(a, b, text, 0.91)], 0
            )

        keep = segment(0.0, 1.0, "bonjour")
        suspect = segment(1.0, 2.0, "정도")
        corrected = segment(0.15, 1.15, " discuter")
        engine = Mock()
        engine.hf_tokenizer = Tokenizer(WordLevel({"mot": 0, "정도": 1}))
        engine.transcribe.side_effect = [
            (iter([keep, suspect]), None),
            (iter([corrected]), None),
        ]
        result = decode_block(engine, np.zeros(48000), "fr", lambda _: None)
        self.assertIs(result[0], keep)
        self.assertEqual(result[1].text, "discuter")
        self.assertAlmostEqual(result[1].words[0].start, 1.0)
        self.assertAlmostEqual(result[1].words[0].end, 2.0)
        (sample,) = engine.transcribe.call_args.args
        self.assertLess(len(sample), 22000)
        self.assertIn(1, engine.transcribe.call_args.kwargs["suppress_tokens"])
        engine.transcribe.reset_mock(side_effect=True)
        engine.transcribe.return_value = (iter([suspect]), None)
        self.assertEqual(
            decode_block(engine, np.zeros(48000), None, lambda _: None), [suspect]
        )
        engine.transcribe.assert_called_once()

    def test_uncertain_word_uses_glossary_and_rejects_unrelated_replacement(self):
        from faster_whisper.transcribe import Segment, Word
        from processing import french_glossary
        import numpy as np

        glossary = french_glossary({"language": "fr", "vocabulary": "Rikai"})
        self.assertIn("frérot", glossary)
        self.assertIn("Rikai", glossary)
        self.assertEqual(french_glossary({"language": "en", "vocabulary": "Rikai"}), "")

        def segment(word, confidence):
            return Segment(
                0,
                0,
                0.0,
                0.7,
                word,
                [],
                -0.1,
                1.0,
                0.0,
                [Word(0.0, 0.7, word, confidence)],
                0,
            )

        heard = segment(" rouffe", 0.42)
        for text, score, expect in (
            (" reuf", 0.89, "reuf"),
            (" reuf", 0.43, "rouffe"),
            (" rive", 0.97, "rouffe"),
        ):
            engine = Mock()
            engine.hf_tokenizer.encode.return_value.ids = [1, 2, 3]
            engine.transcribe.side_effect = [
                (iter([heard]), None),
                (iter([segment(text, score)]), None),
            ]
            result = decode_block(
                engine, np.zeros(16000), "fr", lambda _: None, glossary=glossary
            )
            self.assertEqual(result[0].text.strip(), expect)
            self.assertIn("reuf", engine.transcribe.call_args.kwargs["hotwords"])

    def test_accents_preserve_word_times_and_leave_homophones_alone(self):
        from faster_whisper.transcribe import Segment, Word
        from processing import french_spelling
        words = [Word(0, .4, " Frerot", .8), Word(.45, .8, " somme", .9)]
        sample = Segment(0, 0, 0., .8, "Frerot somme", [], -.2, 1., 0., words, 0)
        result = french_spelling(sample)
        self.assertEqual(result.text, "Frérot somme")
        self.assertEqual(result.words[0], Word(0, .4, " Frérot", .8))
        self.assertEqual(result.words[1], words[1])
