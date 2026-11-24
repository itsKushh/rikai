import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from processing import generate_report


class Chat:
    def __init__(self, complete):
        self.complete = complete

    def tokenize(self, data):
        return data

    def create_chat_completion(self, *, messages, response_format, **options):
        rows = json.loads(messages[1]["content"])
        facts = self.complete(rows, response_format["schema"]["properties"]["faits"])
        if facts is None:
            return {
                "choices": [
                    {"finish_reason": "length", "message": {"content": '{"faits":['}}
                ]
            }
        return {
            "choices": [
                {
                    "finish_reason": "stop",
                    "message": {"content": json.dumps({"faits": facts})},
                }
            ]
        }


def action(row):
    return dict(
        type="Action",
        texte=row["texte"],
        responsable=row["voix"],
        echeance="Lundi",
        preuves=[row["id"]],
    )


class SummaryBlocks(unittest.TestCase):
    def test_truncated_second_block_and_consolidation_keep_all_citations(self):
        completed = []
        events = []
        parts = [dict(id=i, text=f"Action {i}", speaker="Lou") for i in range(1, 37)]

        def reply(rows, schema):
            if "id" in rows[0]:
                ids = [r["id"] for r in rows]
                if min(ids) > 32 and len(rows) > 1:
                    return None
                completed.extend(ids)
                return [action(r) for r in rows]
            if len(rows) > 1:
                return None
            self.fail("A validated singleton does not need another inference")

        result = generate_report(parts, Chat(reply), events.append, constrain=True)
        self.assertEqual(completed, list(range(1, 37)))
        self.assertEqual(result["evidence"], completed)
        self.assertIn("Extraction des faits · bloc 2", events)
        self.assertIn("Reprise du bloc de synthèse", events)
        self.assertIn("Action 36", result["body"])

    def test_oversized_edited_segment_keeps_text_order_and_original_id(self):
        original = ("L'été à 東京. " * 370) + "Fin."
        complete = []

        def reply(rows, schema):
            if "id" not in rows[0]:
                return rows
            if len(rows) > 1 or len(rows[0]["texte"]) > 320 or "maxItems" not in schema:
                return None
            complete.append(rows[0]["texte"])
            return [dict(action(rows[0]), texte="Préparer le plan")]

        result = generate_report(
            [dict(id=91, speaker="Lou", text=original)],
            Chat(reply),
            lambda _: None,
            constrain=True,
        )
        self.assertEqual("".join(complete), original)
        self.assertEqual(result["evidence"], [91])
        self.assertEqual(result["body"].count("Préparer le plan"), 1)

    def test_split_block_rejects_citations_from_a_sibling(self):
        def reply(rows, schema):
            if len(rows) > 1:
                return None
            return [dict(action(rows[0]), preuves=[2])]

        with self.assertRaisesRegex(ValueError, "Preuve"):
            generate_report(
                [dict(id=i, speaker="Lou", text="Envoyer le plan.") for i in (1, 2)],
                Chat(reply),
                lambda _: None,
                constrain=True,
            )

    def test_incomplete_compact_result_is_never_saved(self):
        attempts = []

        def reply(rows, schema):
            attempts.append(schema.get("maxItems"))

        with self.assertRaisesRegex(ValueError, "pas terminé"):
            generate_report(
                [dict(id=1, speaker="Lou", text="Salut.")],
                Chat(reply),
                lambda _: None,
                constrain=True,
            )
        self.assertEqual(attempts, [None, 3])
