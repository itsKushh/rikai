"""Signaler une reprise possible sans retirer ni modifier les paroles."""

from bisect import bisect_left, bisect_right
import re
import wave


def comparable(text):
    return " ".join(re.findall(r"\w+", text.casefold()))


def excerpt(path, start_ms, end_ms):
    import numpy as np

    with wave.open(str(path), "rb") as reader:
        reader.setpos(min(start_ms * 16, reader.getnframes()))
        data = reader.readframes(min(end_ms - start_ms, 2400) * 16)
    # Moyenne glissante avant réduction à 2 kHz. La corrélation est indicative.
    values = np.frombuffer(data, dtype="<i2").astype(np.float64)
    values = values[: len(values) // 8 * 8].reshape(-1, 8).mean(axis=1)
    return values - values.mean() if len(values) else values


def correlation(a, b):
    import numpy as np

    best = 0.0
    for lag in range(-160, 161, 4):
        left = a[max(lag, 0) :]
        right = b[max(-lag, 0) :]
        n = min(len(left), len(right))
        if n < 1500:
            continue
        x, y = left[:n], right[:n]
        norm = float(np.linalg.norm(x) * np.linalg.norm(y))
        if norm > 0:
            best = max(best, float(np.dot(x, y)) / norm)
    return best


def annotate(directory, tracks, segments):
    micros = {t["file"] for t in tracks if t["input"]["source"]["kind"] == "Microphone"}
    remote = sorted(
        (s for s in segments if s["track"] not in micros), key=lambda s: s["start_ms"]
    )
    times = [s["start_ms"] for s in remote]
    for mic in segments:
        if mic["track"] not in micros or mic["end_ms"] - mic["start_ms"] < 900:
            continue
        text = comparable(mic["text"])
        if len(text.split()) < 3:
            continue
        a, b = (
            bisect_left(times, mic["start_ms"] - 500),
            bisect_right(times, mic["start_ms"] + 500),
        )
        for other in remote[a:b]:
            if comparable(other["text"]) != text:
                continue
            x = excerpt(directory / mic["track"], mic["start_ms"], mic["end_ms"])
            y = excerpt(directory / other["track"], other["start_ms"], other["end_ms"])
            score = correlation(x, y)
            if score >= 0.85:
                mic["echo_candidate"] = {
                    "track": other["track"],
                    "start_ms": other["start_ms"],
                    "correlation": score,
                }
                break
