import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from local_summary import compact_repetitions, decode
from processing import generate_report


def chunk(text="", finish=None):
    return {"choices": [{"delta": {"content": text}, "finish_reason": finish}]}


class Chat:
    def __init__(self, complete):
        self.complete = complete

    def tokenize(self, data):
        return data

    def create_chat_completion(self, *, messages, response_format, stream, **options):
        assert stream
        rows = json.loads(messages[1]["content"])
        facts = self.complete(rows, response_format["schema"]["properties"]["faits"])
        if facts is None:
            yield chunk('{"faits":[')
            yield chunk(finish="length")
        else:
            yield chunk(json.dumps({"faits": facts}))
            yield chunk(finish="stop")


def action(row):
    return dict(
        type="Action",
        texte=row["texte"],
        responsable=row["voix"],
        echeance="Lundi",
        preuves=[row["id"]],
    )


class SummaryBlocks(unittest.TestCase):
    def setUp(self):
        self.events = []

    def report(self, parts, reply):
        return generate_report(
            parts,
            Chat(reply),
            lambda stage, **kw: self.events.append(stage),
            constrain=True,
        )

    def test_truncated_second_block_retries_once_and_keeps_all_citations(self):
        completed = []
        parts = [dict(id=i, text=f"Action {i}", speaker="Lou") for i in range(1, 37)]

        def reply(rows, schema):
            ids = [r["id"] for r in rows]
            if ids == list(range(13, 25)):
                return None
            completed.extend(ids)
            return [action(r) for r in rows]

        result = self.report(parts, reply)
        self.assertEqual(completed, list(range(1, 37)))
        self.assertEqual(result["evidence"], completed)
        self.assertFalse(result["partial"])
        self.assertIn("Reprise du bloc 2/3 · partie 2/2", self.events)
        self.assertIn("Action 36", result["body"])

    def test_oversized_edited_segment_keeps_order_and_original_id(self):
        original = " ".join(f"L'été à 東京 {i}." for i in range(370)) + " Fin."
        complete = []

        def reply(rows, schema):
            complete.extend(r["texte"] for r in rows)
            return [dict(action(r), texte="Préparer le plan") for r in rows]

        result = self.report([dict(id=91, speaker="Lou", text=original)], reply)
        self.assertEqual("".join(complete), original)
        self.assertEqual(result["evidence"], [91])
        self.assertEqual(result["body"].count("Préparer le plan"), 1)

    def test_repetitions_are_compacted_only_in_model_input_with_count(self):
        raw = "Avant. " + "L’été à 東京. " * 300 + "Après."
        self.assertEqual(compact_repetitions(raw), "Avant. L’été à 東京. [×300] Après.")
        self.assertEqual(
            compact_repetitions("待つ。待つ。待つ。完了。"), "待つ。 [×3] 完了。"
        )
        ordinary = "Non.  Non.\nNon plus."
        self.assertEqual(compact_repetitions(ordinary), ordinary)
        parts = [dict(id=1, speaker="Lou", text=raw)]
        inputs = []

        def reply(rows, schema):
            inputs.extend(rows)
            return [dict(action(rows[0]), texte="Préparer le plan")]

        self.assertEqual(self.report(parts, reply)["evidence"], [1])
        self.assertEqual(len(inputs), 1)
        self.assertIn("[×300]", inputs[0]["texte"])
        self.assertEqual(parts[0]["text"], raw)

    def test_split_block_rejects_citations_from_a_sibling(self):
        def reply(rows, schema):
            if len(rows) > 1:
                return None
            return [dict(action(rows[0]), preuves=[2])]

        with self.assertRaisesRegex(ValueError, "Preuve"):
            self.report(
                [dict(id=i, speaker="Lou", text="Envoyer le plan.") for i in (1, 2)],
                reply,
            )

    def test_partial_report_marks_only_unresolved_sources_without_recursive_retries(
        self,
    ):
        attempts = []

        def reply(rows, schema):
            ids = [r["id"] for r in rows]
            attempts.append(ids)
            return [action(r) for r in rows] if ids[0] > 6 else None

        parts = [dict(id=i, speaker="Lou", text=f"Action {i}") for i in range(1, 14)]
        result = self.report(parts, reply)
        self.assertEqual([len(ids) for ids in attempts], [12, 6, 6, 1])
        self.assertEqual(result["evidence"], list(range(1, 14)))
        self.assertTrue(result["partial"])
        report, flagged = result["body"].split("## Passages à vérifier")
        self.assertNotIn("Action 3", report)
        self.assertIn("Action 13", report)
        self.assertIn("§6", flagged)
        self.assertNotIn("§7", flagged)

    def test_incomplete_compact_result_is_never_saved(self):
        attempts = []

        def reply(rows, schema):
            attempts.append(schema.get("maxItems"))

        with self.assertRaisesRegex(ValueError, "aucun fait vérifiable"):
            self.report([dict(id=1, speaker="Lou", text="Salut.")], reply)
        self.assertEqual(attempts, [None, 3])

    def test_actual_deltas_drive_progress_and_repeating_facts_close_stream_early(self):
        events = []
        closed = []
        fact = json.dumps(
            {"texte": 'Une } accolade et un "guillemet".', "preuves": [1]}
        )

        class Repeats:
            def create_chat_completion(self, **options):
                try:
                    yield chunk('{"faits":[')
                    for _ in range(5):
                        yield chunk(fact[:-1])
                        yield chunk("},")
                    raise AssertionError("Repetition detection failed")
                finally:
                    closed.append(True)

        with patch("local_summary.time.monotonic", side_effect=range(0, 100, 3)):
            self.assertIsNone(
                decode(
                    Repeats(),
                    {},
                    "bloc",
                    lambda _, **kw: events.append(kw["generated_chars"]),
                )
            )
        self.assertEqual(closed, [True])
        self.assertGreater(len(events), 1)
        self.assertEqual(events, sorted(set(events)))

    def test_time_limit_closes_an_active_but_unproductive_stream(self):
        class Slow:
            def create_chat_completion(self, **opts):
                yield chunk('{"faits":')
                raise AssertionError("Unproductive stream was left open")

        with patch("local_summary.time.monotonic", side_effect=[0, 121]):
            self.assertIsNone(decode(Slow(), {}, "bloc", lambda *a, **kw: None))

    def test_slow_decoder_can_keep_making_distinct_facts(self):
        class Slow:
            def create_chat_completion(self, **opts):
                yield chunk('{"faits":[{"texte":"A"}')
                yield chunk(',{"texte":"B"}')
                yield chunk("]}")
                yield chunk(finish="stop")

        with patch("local_summary.time.monotonic", side_effect=[0, 80, 160, 170]):
            text = decode(Slow(), {}, "bloc", lambda *a, **kw: None)
        self.assertEqual([f["texte"] for f in json.loads(text)["faits"]], ["A", "B"])

    def test_stream_without_terminal_message_does_not_publish(self):
        class Interrupted:
            def create_chat_completion(self, **opts):
                yield chunk('{"faits":[]}')

        self.assertIsNone(decode(Interrupted(), {}, "bloc", lambda *a, **kw: None))
