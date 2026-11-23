"""Empreintes locales. Une suggestion reste distincte d'une identité confirmée."""

import base64
import hashlib
import io
from pathlib import Path
import wave


def model_key(path):
    with open(path, "rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def clip(path, start_ms, end_ms):
    # Limiter à 12 s de parole contiguë. Ne jamais étendre l'extrait hors du tour.
    if end_ms - start_ms < 1500:
        raise ValueError("Choisissez une intervention isolée d'au moins 1,5 seconde")
    with wave.open(str(path), "rb") as reader:
        if (reader.getnchannels(), reader.getframerate(), reader.getsampwidth()) != (
            1,
            16000,
            2,
        ):
            raise ValueError("Format de piste incompatible")
        reader.setpos(start_ms * 16)
        data = reader.readframes(min(end_ms - start_ms, 12000) * 16)
    if len(data) < 48000:
        raise ValueError("Extrait trop court")
    return data


def extractor(path):
    import sherpa_onnx as so

    return so.SpeakerEmbeddingExtractor(
        so.SpeakerEmbeddingExtractorConfig(model=str(path), num_threads=2)
    )


def embedding(engine, data):
    import numpy as np

    samples = np.frombuffer(data, dtype="<i2").astype(np.float32) / 32768
    if np.sqrt(np.mean(samples * samples)) < 0.005:
        raise ValueError("Niveau de parole insuffisant")
    stream = engine.create_stream()
    stream.accept_waveform(16000, samples)
    stream.input_finished()
    if not engine.is_ready(stream):
        raise ValueError("Échantillon vocal insuffisant")
    v = np.asarray(engine.compute(stream), dtype=np.float32)
    norm = float(np.linalg.norm(v))
    if not np.isfinite(norm) or norm == 0:
        raise ValueError("Empreinte invalide")
    return (v / norm).tolist()


def enroll(p, progress):
    if p.get("consent") is not True:
        raise ValueError("Autorisation explicite requise")
    progress("Analyse de l’extrait autorisé")
    seg = p["segment"]
    data = clip(Path(p["file"]), seg["start_ms"], seg["end_ms"])
    model = Path(p["settings"]["embedding"])
    vec = embedding(extractor(model), data)
    buf = io.BytesIO()
    with wave.open(buf, "wb") as out:
        out.setparams((1, 2, 16000, 0, "NONE", ""))
        out.writeframes(data)
    return dict(
        name=p["name"],
        model=model_key(model),
        embedding=vec,
        samples=base64.b64encode(buf.getvalue()).decode("ascii"),
    )


def candidate(vector, profiles, threshold=0.77, margin=0.10):
    import numpy as np

    scores = []
    for p in profiles:
        v = np.asarray(p["embedding"])
        if v.shape != (len(vector),):
            continue
        score = float(np.dot(vector, v))
        if np.isfinite(score):
            scores.append((score, p["id"]))
    scores.sort(reverse=True)
    if (
        not scores
        or scores[0][0] < threshold
        or len(scores) > 1
        and scores[0][0] - scores[1][0] < margin
    ):
        return None
    return dict(profile=scores[0][1], score=scores[0][0])


def suggest(directory, settings, segments, profiles):
    path = Path(settings["embedding"])
    key = model_key(path)
    compatible = [p for p in profiles if p["model"] == key]
    if not compatible:
        return {}
    engine = extractor(path)
    suggestions = {}
    for label in sorted(
        {
            s["speaker"]
            for s in segments
            if s["speaker"].startswith("Intervenant ")
            and s["speaker"] != "Intervenant incertain"
        }
    ):
        options = sorted(
            (s for s in segments if s["speaker"] == label),
            key=lambda s: s["end_ms"] - s["start_ms"],
            reverse=True,
        )
        votes = []
        for s in options[:3]:
            try:
                v = embedding(
                    engine, clip(directory / s["track"], s["start_ms"], s["end_ms"])
                )
            except ValueError:
                continue
            votes.append(candidate(v, compatible))
        if len(votes) >= 2 and all(votes) and len({v["profile"] for v in votes}) == 1:
            suggestions[label] = votes[0]
    return suggestions
