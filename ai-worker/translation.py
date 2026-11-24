"""Local M2M100 inference. Only the selected passage reaches this process."""

from functools import lru_cache
from pathlib import Path
import re

LANGUAGES = frozenset(
    """af am ar ast az ba be bg bn br bs ca ceb cs cy da de el en es et fa ff fi fr fy ga gd gl gu ha he hi hr ht hu hy id ig ilo is it ja jv ka kk km kn ko lb lg ln lo lt lv mg mk ml mn mr ms my ne nl no ns oc or pa pl ps pt ro ru sd si sk sl so sq sr ss su sv sw ta th tl tn tr uk ur uz vi wo xh yi yo zh zu""".split()
)


# M2M100 was trained mainly on written text. Expand distinctive spoken forms
# on a working copy. Common words such as "fret" need a vocative context.
FRENCH_ORAL = {
    "frérot": "mon frère",
    "frerot": "mon frère",
    "fréro": "mon frère",
    "reuf": "frère",
    "reufs": "frères",
    "frérots": "frères",
    "meuf": "fille",
    "meufs": "filles",
    "teuf": "fête",
    "teufs": "fêtes",
    "chelou": "bizarre",
    "relou": "pénible",
    "daron": "père",
    "daronne": "mère",
    "keuf": "policier",
    "keufs": "policiers",
    "taffer": "travailler",
}
ORAL_WORD = re.compile(r"\b(?:" + "|".join(FRENCH_ORAL) + r")\b", re.I)


def translation_input(text, source):
    if source != "fr":
        return text
    text = re.sub(
        r"\b(salut|merci|bonjour|écoute|wesh)[ ,]+fret(?=\s*(?:[!.?,]|$))",
        r"\1 frérot",
        text,
        flags=re.I,
    )
    text = re.sub(
        r"\b(mon|ton|son|un|le|ce) (?:fr[ée]rot?|reuf)\b", r"\1 frère", text, flags=re.I
    )
    text = re.sub(
        r"\b(suis|est|sont|étais|était) vénère\b", r"\1 en colère", text, flags=re.I
    )
    text = re.sub(r"^wesh(?=[ ,!?]|$)", "Salut", text, flags=re.I)
    return ORAL_WORD.sub(lambda m: FRENCH_ORAL[m[0].lower()], text)


@lru_cache(maxsize=1)
def detector():
    from py3langid.langid import LanguageIdentifier, MODEL_FILE

    model = LanguageIdentifier.from_pickled_model(MODEL_FILE)
    model.set_languages(sorted(set(model.nb_classes) & LANGUAGES))
    return model


@lru_cache(maxsize=1)
def translation_model(folder, threads=0):
    import ctranslate2
    import sentencepiece as spm

    tokenizer = spm.SentencePieceProcessor(
        model_file=str(Path(folder) / "sentencepiece.bpe.model")
    )
    engine = ctranslate2.Translator(
        str(folder),
        device="cpu",
        compute_type="int8",
        inter_threads=1,
        intra_threads=threads,
    )
    return tokenizer, engine


def translate(p, progress):
    text, target = p["text"].strip(), p["target"]
    if target not in LANGUAGES or not text:
        raise ValueError("Langue ou passage invalide")
    source = p.get("source")
    if source not in LANGUAGES:
        source = (
            "fr"
            if text.casefold().strip(" .!?") in FRENCH_ORAL
            else detector().classify(p.get("context") or text)[0]
        )
    if source == target:
        return {"text": text, "source": source, "target": target}
    progress("Traduction du passage")
    tokenizer, engine = translation_model(p["folder"], p.get("threads", 0))
    pieces = tokenizer.encode(translation_input(text, source), out_type=str)
    # Split long edited passages at a word boundary; never silently truncate them.
    chunks = []
    while pieces:
        limit = min(len(pieces), 480)
        if limit < len(pieces):
            limit = next(
                (i for i in range(limit, limit // 2, -1) if pieces[i].startswith("▁")),
                limit,
            )
        chunks.append([f"__{source}__", *pieces[:limit], "</s>"])
        pieces = pieces[limit:]
    results = engine.translate_batch(
        chunks,
        target_prefix=[[f"__{target}__"] for _ in chunks],
        beam_size=4,
        max_decoding_length=768,
    )
    output = " ".join(
        tokenizer.decode([w for w in r.hypotheses[0] if w != f"__{target}__"])
        for r in results
    ).strip()
    if not output:
        raise ValueError("La traduction est vide")
    return {"text": output, "source": source, "target": target}
