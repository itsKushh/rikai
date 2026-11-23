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
            return Segment(0, 0, a, b, text, [], -0.1, 1.0, 0.0, [Word(a, b, text, 0.91)], 0)
        keep = segment(0., 1., "bonjour")
        suspect = segment(1., 2., "정도")
        corrected = segment(.15, 1.15, " discuter")
        engine = Mock()
        engine.hf_tokenizer = Tokenizer(WordLevel({"mot": 0, "정도": 1}))
        engine.transcribe.side_effect = [(iter([keep, suspect]), None), (iter([corrected]), None)]
        result = decode_block(engine, np.zeros(48000), "fr", lambda _: None)
        self.assertIs(result[0], keep)
        self.assertEqual(result[1].text, "discuter")
        self.assertAlmostEqual(result[1].words[0].start, 1.)
        self.assertAlmostEqual(result[1].words[0].end, 2.)
        sample, = engine.transcribe.call_args.args
        self.assertLess(len(sample), 22000)
        self.assertIn(1, engine.transcribe.call_args.kwargs["suppress_tokens"])
        engine.transcribe.reset_mock(side_effect=True)
        engine.transcribe.return_value = (iter([suspect]), None)
        self.assertEqual(decode_block(engine, np.zeros(48000), None, lambda _: None), [suspect])
        engine.transcribe.assert_called_once()
