import json
import re
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from local_summary import compact_repetitions, decode
from summary import generate_report, read_markdown, turns, verified, windows


def chunk(text="", finish=None):
    return {"choices": [{"delta": {"content": text}, "finish_reason": finish}]}


def ids_of(data):
    # Source labels come before the speaker and text on each chronological line.
    return [
        int(i)
        for line in data.splitlines()
        for i in re.findall(r"§(\d+)", line.split(" : ", 1)[0])
    ]


def fact(text, *ids, anchor="Le quai"):
    return {"appui": anchor, "texte": text, "preuves": list(ids)}


def report(ids, anchor="Le quai"):
    return {
        "sujets": [fact("Le quai est en travaux.", ids[0], ids[-1], anchor=anchor)],
        "complements": [],
        "bref": "Ils discutent des travaux du quai.",
    }


class Chat:
    def __init__(self, reply, cost=1):
        self.reply = reply
        self.cost = cost
        self.calls = []

    def tokenize(self, data):
        return data * self.cost

    def create_chat_completion(self, *, messages, response_format=None, stream, **opts):
        assert stream
        data = (
            messages[-1]["content"].split("\n", 1)[1].rsplit("\nFin des données.", 1)[0]
        )
        final = (
            response_format is None or "bref" in response_format["schema"]["properties"]
        )
        self.calls.append((final, data))
        value = self.reply(data, final)
        if value is None:
            yield chunk('{"')
            yield chunk(finish="length")
        else:
            if response_format is None and isinstance(value, dict):
                lines = ["## En bref", value["bref"], "## Sujets abordés"]
                for point in value["sujets"]:
                    refs = ", ".join(f"§{i}" for i in point["preuves"])
                    lines.append(f"- {point['texte']} ({refs})")
                for extra in value.get("complements", []):
                    lines.extend(
                        [
                            f"## {extra['type']}",
                            f"- « {extra['citation']} » (§{extra['id']})",
                        ]
                    )
                yield chunk("\n".join(lines))
            else:
                yield chunk(value if isinstance(value, str) else json.dumps(value))
            yield chunk(finish="stop")


class SummaryBlocks(unittest.TestCase):
    def setUp(self):
        self.events = []

    def generate(self, parts, model):
        return generate_report(
            parts, model, lambda stage, **kw: self.events.append(stage), constrain=True
        )

    def test_global_input_keeps_order_speakers_last_topic_and_one_call(self):
        parts = [
            dict(id=90 + i, speaker=["Mia", "Lou"][i % 2], text=f"Phrase {i}.")
            for i in range(80)
        ]
        model = Chat(lambda data, final: report(ids_of(data), "Phrase 0"))
        result = self.generate(parts, model)
        self.assertEqual(len(model.calls), 1)
        final, data = model.calls[0]
        self.assertTrue(final)
        self.assertEqual(ids_of(data), [p["id"] for p in parts])
        for p, line in zip(parts, data.splitlines()):
            self.assertIn(p["text"], line)
            self.assertIn(p["speaker"], line)
        self.assertEqual(result["evidence"], [90, 169])
        self.assertTrue(result["body"].startswith("## En bref"))
        self.assertIn("## Sujets abordés", result["body"])
        self.assertNotIn("Suites convenues", result["body"])
        self.assertNotIn("Non précisé", result["body"])

    def test_optional_genuine_commitment_is_rendered_once(self):
        def reply(data, final):
            value = report([5, 8], "Le plan")
            value["complements"] = [
                {
                    "type": "Suites convenues",
                    "citation": "Je vais envoyer le plan mardi.",
                    "id": 8,
                },
                {"type": "Difficultés", "citation": "La salle était fermée.", "id": 5},
            ]
            return value

        result = self.generate(
            [
                dict(id=5, speaker="Sam", text="Le plan est prêt."),
                dict(id=8, speaker="Lou", text="Je vais envoyer le plan mardi."),
            ],
            Chat(reply),
        )
        self.assertIn(
            "## Suites convenues\n\n- Lou — « Je vais envoyer le plan mardi. » (§8)",
            result["body"],
        )

        self.assertNotIn("La salle était fermée", result["body"])

    def test_large_recordings_use_all_windows_then_a_global_synthesis(self):
        parts = [
            dict(id=i, speaker="Lou", text=f"Evidence {i}. " + ("long texte " * 8))
            for i in range(1, 131)
        ]

        def reply(data, final):
            ids = ids_of(data)
            return (
                report(ids, "Evidence 1")
                if final
                else {
                    "faits": [
                        fact(
                            "Le canal traverse la ville.",
                            ids[0],
                            ids[-1],
                            anchor=f"Evidence {ids[0]}",
                        )
                    ]
                }
            )

        model = Chat(reply)
        result = self.generate(parts, model)
        self.assertGreater(len(model.calls), 2)
        self.assertTrue(model.calls[-1][0])
        self.assertEqual(
            [i for final, data in model.calls if not final for i in ids_of(data)],
            list(range(1, 131)),
        )
        self.assertEqual(result["evidence"], [1, 130])
        self.assertFalse(result["partial"])

    def test_partial_notes_identify_only_unread_sources_and_still_have_an_overview(
        self,
    ):
        parts = [
            dict(id=i, speaker="Lou", text=f"Notes {i}. " + ("article " * 18))
            for i in range(1, 85)
        ]
        missed = []

        def reply(data, final):
            ids = ids_of(data)
            if final:
                return report(ids, "Notes 1")
            if ids[0] > 1 and ids[0] < 65:
                missed.extend(ids)
                return None
            return {
                "faits": [
                    fact(
                        "Le canal traverse la ville.",
                        ids[0],
                        ids[-1],
                        anchor=f"Notes {ids[0]}",
                    )
                ]
            }

        result = self.generate(parts, Chat(reply))
        self.assertTrue(result["partial"])
        self.assertTrue(result["body"].startswith("## En bref"))
        self.assertTrue(set(missed) <= set(result["evidence"]))
        self.assertIn("## Passages à vérifier", result["body"])

    def test_unusable_global_response_keeps_checked_notes_but_never_claims_completion(
        self,
    ):
        def reply(data, final):
            if final:
                return report([9999])
            return {
                "faits": [fact("La date prévue est confirmée.", 8, anchor="Une idée")]
            }

        model = Chat(reply)
        r = self.generate([dict(id=8, speaker="Eve", text="Une idée.")], model)
        self.assertTrue(r["body"].startswith("## Synthèse partielle"))
        self.assertTrue(r["partial"])
        self.assertEqual(r["evidence"], [8])
        self.assertEqual(len(model.calls), 3)

    def test_anchor_can_resolve_a_neighbouring_id_but_must_not_escape_its_block(self):
        passages = {
            5: {"speaker": "Ana", "text": "Je travaille à la bibliothèque."},
            8: {"speaker": "Sam", "text": "La toiture est endommagée."},
        }
        data = {
            "faits": [
                fact(
                    "Sam signale que le toit est endommagé.",
                    5,
                    anchor="toiture est endommagée",
                )
            ]
        }
        content = json.dumps(data)
        self.assertEqual(verified(content, {5, 8}, False, 5, passages)[0].refs, [8])
        self.assertIsNone(verified(content, {5}, False, 5, passages))

        data["faits"][0]["appui"] = '§8 "Sam" : "La toiture est endommagée."'
        grounded = verified(json.dumps(data), {5, 8}, False, 5, passages)
        self.assertEqual(grounded[0].refs, [8])
        self.assertIn("La toiture est endommagée.", grounded[0].support)

    def test_unfinished_sentences_do_not_become_a_report(self):
        data = report([4], "le quai")
        data["bref"] = "Le"
        passages = {4: {"speaker": "Lou", "text": "Le quai est fermé."}}
        self.assertIsNone(verified(json.dumps(data), {4}, True, 5, passages))

    def test_markdown_checks_references_joins_wrapped_items_and_discards_outro(self):
        passages = {
            2: {"text": "Le marché ouvre lundi.", "speaker": "Ana"},
            8: {"text": "D’accord.", "speaker": "Lou"},
        }
        content = """## En bref
Deux proches discutent de l’ouverture du marché (§2).
## Sujets abordés
- Ana parle de la date
  d’ouverture du marché (§2–8).

Merci de votre attention.
## Suites convenues
- Lou va acheter des pommes (§8).
"""
        points = read_markdown(content, set(passages), passages)
        self.assertEqual([p.section for p in points], ["En bref", "Sujets abordés"])
        self.assertNotIn("§", points[0].text)
        self.assertEqual(points[1].refs, [2, 8])
        self.assertEqual(points[1].text, "Ana parle de la date d’ouverture du marché.")
        styled = (
            content.replace("## En bref\n", "**En bref** : ")
            .replace("## Sujets abordés", "**Sujets abordés :**")
            .replace("d’ouverture du marché (§2–8)", "d’ouverture du *marché* (§2–8)")
        )
        self.assertEqual(read_markdown(styled, set(passages), passages), points)
        self.assertIsNone(
            read_markdown(content.replace("§2–8", "§2, 99"), set(passages), passages)
        )

    def test_invalid_markdown_retries_with_grounded_schema(self):
        recovered = {
            "sujets": [{"texte": "Le quai est en travaux.", "preuves": [4]}],
            "bref": "Ils discutent des travaux du quai.",
        }
        replies = iter(["## En bref\nUn texte sans sujet ni source.", recovered])
        model = Chat(lambda *args: next(replies))
        result = self.generate(
            [dict(id=4, speaker="Lou", text="Le quai est en travaux.")], model
        )
        self.assertFalse(result["partial"])
        self.assertEqual(len(model.calls), 2)
        self.assertEqual(result["evidence"], [4])

    def test_edited_passage_is_split_without_losing_words_or_ids(self):
        original = " ".join(f"L’été à 東京 {i}." for i in range(1100))
        packed = list(windows([([9], "Lou", original)], Chat(None), 1000))
        self.assertGreater(len(packed), 1)
        pieces = [
            json.loads(line.split(" : ", 1)[1])
            for data, refs in packed
            for line in data.splitlines()
        ]
        self.assertEqual("".join(pieces), original)
        self.assertTrue(all(ids == {9} for _, ids in packed))

    def test_repetition_count_keeps_source_and_speaker_boundaries(self):
        raw = "Avant. " + "L’été à 東京. " * 300 + "Après."
        self.assertEqual(compact_repetitions(raw), "Avant. L’été à 東京. [×300] Après.")
        parts = [
            dict(id=i, speaker=s, text="Oui.")
            for i, s in enumerate(["Lou", "Lou", "Lou", "Sam", "Lou"], 1)
        ]
        rows = list(turns(parts))
        self.assertEqual(
            rows,
            [([1], "Lou", "Oui. [×3]"), ([4], "Sam", "Oui."), ([5], "Lou", "Oui.")],
        )
        self.assertEqual(parts[0]["text"], "Oui.")

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
                    {"response_format": {"type": "json_object"}},
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
                yield chunk(" ")
                raise AssertionError("Unproductive stream was left open")

        with patch("local_summary.time.monotonic", side_effect=[0, 1, 122]):
            self.assertIsNone(decode(Slow(), {}, "bloc", lambda *a, **kw: None))

    def test_slow_decoder_can_keep_making_distinct_facts(self):
        class Slow:
            def create_chat_completion(self, **opts):
                yield chunk('{"faits":[{"texte":"A"}')
                yield chunk(',{"texte":"B"}')
                yield chunk("]}")
                yield chunk(finish="stop")

        with patch("local_summary.time.monotonic", side_effect=[0, 80, 160, 170]):
            text = decode(
                Slow(),
                {"response_format": {"type": "json_object"}},
                "bloc",
                lambda *a, **kw: None,
            )
        self.assertEqual([f["texte"] for f in json.loads(text)["faits"]], ["A", "B"])

    def test_stream_without_terminal_message_does_not_publish(self):
        class Interrupted:
            def create_chat_completion(self, **opts):
                yield chunk('{"faits":[]}')

        self.assertIsNone(decode(Interrupted(), {}, "bloc", lambda *a, **kw: None))
