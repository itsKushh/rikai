"""Local M2M100 inference. Only the selected passage reaches this process."""

from pathlib import Path

LANGUAGES = frozenset(
    """af am ar ast az ba be bg bn br bs ca ceb cs cy da de el en es et fa ff fi fr fy ga gd gl gu ha he hi hr ht hu hy id ig ilo is it ja jv ka kk km kn ko lb lg ln lo lt lv mg mk ml mn mr ms my ne nl no ns oc or pa pl ps pt ro ru sd si sk sl so sq sr ss su sv sw ta th tl tn tr uk ur uz vi wo xh yi yo zh zu""".split()
)


def translate(p, progress):
    import ctranslate2
    import sentencepiece as spm

    text, target = p["text"].strip(), p["target"]
    if target not in LANGUAGES or not text:
        raise ValueError("Langue ou passage invalide")
    source = p.get("source")
    if source not in LANGUAGES:
        from py3langid.langid import LanguageIdentifier, MODEL_FILE

        detector = LanguageIdentifier.from_pickled_model(MODEL_FILE)
        detector.set_languages(sorted(set(detector.nb_classes) & LANGUAGES))
        source = detector.classify(text)[0]
    if source == target:
        return {"text": text, "source": source, "target": target}
    folder = Path(p["folder"])
    progress("Traduction du passage")
    tokenizer = spm.SentencePieceProcessor(
        model_file=str(folder / "sentencepiece.bpe.model")
    )
    engine = ctranslate2.Translator(
        str(folder), device="cpu", compute_type="int8", inter_threads=1
    )
    pieces = tokenizer.encode(text, out_type=str)
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
