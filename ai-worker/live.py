"""Read the committed PCM frames of each capture track while WASAPI is recording."""

from pathlib import Path
import time
import wave

from processing import (
    EAST_ASIAN_SCRIPT,
    french_glossary,
    french_spelling,
    local_path,
    nearby_terms,
    reread_french_passage,
)
from resources import apply_limits, gpu_timeslice
from nvidia_runtime import activate as activate_cuda
from translation import LANGUAGES, translate, translation_model

RATE = 16000
BLOCK = RATE // 50


def read_frames(path, start, limit=RATE * 8):
    import numpy as np

    try:
        with path.open("rb") as stream:
            header = stream.read(8)
            # Hound writes a zero RIFF size until TrackWriter's first flush.
            if len(header) < 8 or header == b"RIFF" + bytes(4):
                return None
            stream.seek(0)
            with wave.open(stream, "rb") as wav:
                count = wav.getnframes() - start
                if count <= 0 or limit <= 0:
                    return None
                if (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) != (
                    RATE,
                    1,
                    2,
                ):
                    raise ValueError("Format de piste incompatible")
                wav.setpos(start)
                return np.frombuffer(wav.readframes(min(count, limit)), dtype="<i2")
    except (FileNotFoundError, EOFError):
        return None


class Track:
    def __init__(self, index, directory):
        import numpy as np

        self.index = index
        self.path = directory / f"piste-{index + 1}.wav"
        self.pcm = np.empty(0, dtype=np.int16)
        self.start = self.partial = 0
        self.revision = ""

    def step(self, engine, config, finished, report):
        import numpy as np

        incoming = read_frames(
            self.path, self.start + len(self.pcm), 8 * RATE - len(self.pcm)
        )
        if incoming is not None and len(incoming):
            self.pcm = np.concatenate((self.pcm, incoming))
        if not len(self.pcm):
            return False

        # Prefer a pause instead of cutting the final consonant. Trim leading
        # silence in bounded batches so an idle source needs no ASR inference.
        samples = self.pcm[: 8 * RATE]
        blocks = (
            samples[: len(samples) // BLOCK * BLOCK]
            .reshape(-1, BLOCK)
            .astype("float32")
        )
        active = np.flatnonzero(np.mean(blocks * blocks, axis=1) > 34**2)
        if not len(active):
            consumed = len(samples) if finished else max(0, len(samples) - 3 * BLOCK)
            self.start += consumed
            self.pcm = self.pcm[consumed:]
            return bool(consumed)
        lead = max(0, (int(active[0]) - 5) * BLOCK)
        if lead and not self.partial:
            self.start += lead
            self.pcm = self.pcm[lead:]
            return True
        gaps = np.flatnonzero((np.diff(active) >= 20) & (active[:-1] >= 20))
        end = len(samples)
        tail = len(samples) - (int(active[-1]) + 1) * BLOCK
        endpoint = finished or tail >= RATE * 0.42 or len(samples) >= RATE * 6
        if len(gaps):
            end = (int(active[gaps[0]]) + 8) * BLOCK
            endpoint = True
        if not endpoint and len(samples) - self.partial < RATE * 1.25:
            return False

        data = samples[:end].astype("float32") / 32768
        with gpu_timeslice(config["duty"]):
            segments, info = engine.transcribe(
                data,
                language=config["source"],
                beam_size=3,
                temperature=0,
                word_timestamps=True,
                vad_filter=True,
                vad_parameters={"min_silence_duration_ms": 350},
                condition_on_previous_text=False,
            )
            parts = list(segments)
        source = config["source"] or info.language
        if endpoint and source == "fr":
            for i, part in enumerate(parts):
                hints = nearby_terms(part.words or [], config["glossary"])
                unexpected = bool(EAST_ASIAN_SCRIPT.search(part.text))
                if hints or unexpected:
                    with gpu_timeslice(config["duty"]):
                        parts[i] = reread_french_passage(
                            engine, data, part, hints, unexpected
                        )
        # At the time limit carry the unfinished word into the next window.
        if (
            not finished
            and not len(gaps)
            and tail < RATE * 0.42
            and len(samples) >= RATE * 6
        ):
            words = [
                w
                for part in parts
                for w in (part.words or [])
                if w.end <= end / RATE - 0.35
            ]
            if words:
                text = "".join(w.word for w in words).strip()
                end = max(BLOCK, round(words[-1].end * RATE))
            else:
                text = " ".join(p.text.strip() for p in parts)
        else:
            text = " ".join(
                (french_spelling(p) if source == "fr" else p).text.strip()
                for p in parts
            )
        payload = {"track": self.index, "start": self.start, "text": text}
        if text != self.revision:
            report("Écoute en direct", live=payload)
            self.revision = text
        if endpoint:
            if text:
                result = translate(
                    {
                        "text": text,
                        "source": source,
                        "target": config["target"],
                        "folder": config["folder"],
                        "threads": config["threads"],
                    },
                    lambda _: None,
                )
                report(
                    "Écoute en direct", live=dict(payload, translated=result["text"])
                )
            self.start += end
            self.pcm = self.pcm[end:]
            self.partial = 0
            self.revision = ""
        else:
            self.partial = len(samples)
        return True


def listen(p, report):
    settings = p["settings"]
    threads = apply_limits(settings)
    import onnxruntime

    onnxruntime.disable_telemetry_events()
    from faster_whisper import WhisperModel

    root = Path(p["directory"])
    if p["target"] not in LANGUAGES:
        raise ValueError("Langue ou passage invalide")
    device = settings.get("device", "cpu")
    report("Chargement du modèle de transcription", phase="load")
    if device == "cuda":
        activate_cuda()
    engine = WhisperModel(
        str(local_path(settings["whisper"], directory=True)),
        device=device,
        compute_type="float16" if device == "cuda" else "int8",
        cpu_threads=threads,
        local_files_only=True,
    )
    config = dict(
        source=settings.get("language") or None,
        target=p["target"],
        folder=settings["translation"],
        threads=threads,
        duty=settings.get("gpu_duty", 100) if device == "cuda" else 100,
        glossary=french_glossary(settings),
    )
    translation_model(config["folder"], threads)
    report("Écoute en direct", live={"ready": True})
    tracks = [Track(i, root) for i in range(p["tracks"])]
    last = time.monotonic()
    while True:
        finished = (root / "finalise").exists()
        worked = [track.step(engine, config, finished, report) for track in tracks]
        if finished and not any(worked):
            return {}
        if not any(worked):
            time.sleep(0.12)
        if time.monotonic() - last > 15:
            report("Écoute en direct")
            last = time.monotonic()
