"""Read the committed PCM frames of each capture track while WASAPI is recording."""

from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path
from threading import Event, Lock
from dataclasses import dataclass
import time
import wave

from processing import (
    EAST_ASIAN_SCRIPT,
    cuda_unavailable,
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


def decode(engine, inference_audio, config, quality, vad=True):
    with gpu_timeslice(config["duty"]):
        segments, info = engine.transcribe(
            inference_audio,
            language=config["source"],
            beam_size=3 if quality else 1,
            temperature=0,
            word_timestamps=quality,
            without_timestamps=not quality,
            vad_filter=vad,
            vad_parameters={"min_silence_duration_ms": 350},
            condition_on_previous_text=False,
        )
        parts = list(segments)
    source = config["source"] or info.language
    if quality and source == "fr":
        for i, part in enumerate(parts):
            hints = nearby_terms(part.words or [], config["glossary"])
            unexpected = bool(EAST_ASIAN_SCRIPT.search(part.text))
            if hints or unexpected:
                with gpu_timeslice(config["duty"]):
                    parts[i] = reread_french_passage(
                        engine, inference_audio, part, hints, unexpected
                    )
    if quality and source == "fr":
        parts = [french_spelling(part) for part in parts]
    return parts, source


def decoded_text(parts, end, forced):
    text = " ".join(p.text.strip() for p in parts).strip()
    if any(not part.words for part in parts):
        return text, [(p.start, p.end, " " + p.text.strip()) for p in parts], end
    words = [(w.start, w.end, w.word) for part in parts for w in (part.words or [])]
    if forced:
        kept = [w for w in words if w[1] <= end / RATE - 0.35]
        if kept:
            return (
                "".join(w[2] for w in kept).strip(),
                kept,
                max(BLOCK, round(kept[-1][1] * RATE)),
            )
    return text, words, end


def recover_speech(engine, data, config, spans):
    # Retry once if the independent speaker segmenter found speech that the
    # first pass missed. Exclude leading/trailing soundtrack from this decode.
    solo = [(a, b) for a, b, who in spans if who != "overlap"]
    if sum(b - a for a, b in solo) < 0.35:
        return None
    start = max(0, round((solo[0][0] - 0.12) * RATE))
    stop = min(len(data), round((solo[-1][1] + 0.12) * RATE))
    from live_voices import level_detected_speech

    sample = level_detected_speech(data[start:stop])
    parts, source = decode(engine, sample, config, True, vad=False)
    clear = [
        part for part in parts if part.words and part.no_speech_prob < 0.65
        and part.avg_logprob > -1.0
        and sum(w.probability for w in part.words) / len(part.words) >= 0.5
    ]
    text, words, _ = decoded_text(clear, stop - start, False)
    if text:
        offset = start / RATE
        return text, [(a + offset, b + offset, w) for a, b, w in words], source
    return None

@dataclass
class Preview:
    text: str
    source: str
    quality: bool
    stop: Event
    future: Future | None = None


class Track:
    def __init__(self, index, directory):
        import numpy as np

        self.index = index
        self.path = directory / f"piste-{index + 1}.wav"
        self.pcm = np.empty(0, dtype=np.int16)
        self.start = self.partial = 0
        self.revision = None
        self.speculation = None
        self.preview = None
        self.voice_tail = np.empty(0, dtype="float32")
        self.voice_end = 0
        self.preview_gate = Lock()
        self.last_audio = time.monotonic()

    @staticmethod
    def translate(text, source, config, on_token=None):
        with gpu_timeslice(config["duty"] if config["mt_device"] == "cuda" else 100):
            return translate(
                dict(
                    text=text,
                    source=source,
                    target=config["target"],
                    folder=config["folder"],
                    threads=config["mt_threads"],
                    device=config["mt_device"],
                ),
                lambda _: None,
                on_token=on_token,
            )["text"]

    def take_preview(self):
        with self.preview_gate:
            previous, self.preview = self.preview, None
            if previous:
                previous.stop.set()
        return previous

    def preview_translation(self, payload, source, quality, config, report):
        old = self.take_preview()
        if old:
            old.future.cancel()
        report("Écoute en direct", live=payload)
        text = payload["text"]
        if not text:
            return
        job = Preview(text, source, quality, Event())
        self.preview = job

        def run():
            tokens = []
            last = 0
            shown = ""

            def publish(text):
                nonlocal shown
                if text and shown != text:
                    with self.preview_gate:
                        if not job.stop.is_set():
                            report(
                                "Écoute en direct", live=dict(payload, translated=text)
                            )
                            shown = text

            def on_token(token, tokenizer):
                nonlocal last
                if job.stop.is_set():
                    return True
                if token not in ("</s>", f"__{config['target']}__"):
                    tokens.append(token)
                    now = time.monotonic()
                    if now - last >= 0.06:
                        publish(tokenizer.decode(tokens).strip())
                        last = now
                return False

            output = self.translate(text, source, config, None if quality else on_token)
            publish(output)
            return output

        job.future = config["mt_pool"].submit(run)

    def voice_turns(self, data, config):
        import numpy as np

        voices = config["voices"]
        gap = self.start - self.voice_end
        history = self.voice_tail
        if len(history) and 0 <= gap <= RATE * 0.8:
            history = np.concatenate((history, np.zeros(gap, dtype="float32")))
            room = min(RATE * 2, voices.size - len(data))
            history = history[-room:] if room > 0 else history[:0]
        else:
            history = history[:0]
        window = np.concatenate((history, data))
        self.voice_tail = window[-RATE * 2 :].copy()
        self.voice_end = self.start + len(data)
        return config["voice_pool"].submit(
            voices.turns, window, len(history), self.index, self.start - len(history)
        )

    def confirm(self, payload, groups, source, speech_end, config, report):
        previous = self.take_preview()
        if previous and not (
            previous.quality
            and any(
                previous.text == phrase and previous.source == source
                for _, phrase in groups
            )
        ):
            previous.future.cancel()
        for i, (speaker, phrase) in enumerate(groups):
            row = dict(
                payload,
                part=i,
                text=phrase,
                speaker=speaker,
                final=True,
                speech_end=speech_end,
            )
            if phrase:
                if not self.revision or phrase != self.revision[0] or i:
                    report("Écoute en direct", live=dict(row, final=False))
                if (
                    previous
                    and previous.quality
                    and (previous.text, previous.source) == (phrase, source)
                ):
                    row["translated"] = previous.future.result()
                else:
                    row["translated"] = self.translate(phrase, source, config)
                if speaker is not None:
                    row["suggestion"] = config["voices"].memory.suggestion(speaker)
            report("Écoute en direct", live=row)

    def step(self, engine, config, finished, report):
        import numpy as np

        incoming = read_frames(
            self.path, self.start + len(self.pcm), 8 * RATE - len(self.pcm)
        )
        if incoming is not None and len(incoming):
            self.last_audio = time.monotonic()
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
        gaps = np.flatnonzero((np.diff(active) >= 18) & (active[:-1] >= 20))
        end = len(samples)
        tail = len(samples) - (int(active[-1]) + 1) * BLOCK
        # Loopback devices can stop sending packets as soon as playback stops.
        # Commit only real samples so a later packet keeps its original timestamp.
        starved = incoming is None and time.monotonic() - self.last_audio >= 0.4
        endpoint = (
            finished or starved or tail >= RATE * 0.32 or len(samples) >= RATE * 6
        )
        if len(gaps):
            end = (int(active[gaps[0]]) + 8) * BLOCK
            endpoint = True
        voiced_end = (int(active[-1]) + 1) * BLOCK
        forced = (
            (not finished or len(samples) == 8 * RATE)
            and not starved
            and not len(gaps)
            and tail < RATE * 0.32
            and len(samples) >= RATE * 6
        )
        # Start the quality pass during a possible pause. Only commit it after the
        # full endpoint interval has elapsed without a new voiced frame.
        anticipate = not endpoint and tail >= 0.10 * RATE and voiced_end >= RATE * 0.45
        quality = endpoint or anticipate
        reusable = (
            self.speculation is not None
            and self.speculation[0] == voiced_end
            and not len(gaps)
        )
        interval = 0.82 if self.partial else 0.40
        if not endpoint and (
            anticipate
            and reusable
            or not anticipate
            and len(samples) - self.partial < RATE * interval
        ):
            return False

        data = samples[:end].astype("float32") / 32768
        turns = None
        if endpoint and not forced and config["voices"] is not None:
            turns = self.voice_turns(data, config)
        if endpoint and reusable:
            _, parts, source = self.speculation
        else:
            inference_audio = (
                np.pad(data, (0, max(0, voiced_end + 16 * BLOCK - end)))
                if anticipate
                else data
            )
            parts, source = decode(engine, inference_audio, config, quality)
        if anticipate:
            self.speculation = voiced_end, parts, source
        text, timed_words, end = decoded_text(parts, end, forced)
        payload = {
            "track": self.index,
            "start": self.start,
            "text": text,
            "final": False,
        }
        if not endpoint:
            # Stream the first words greedily, then prepare full beam-search
            # translations that can be reused if the final source agrees exactly.
            full_translation = quality or bool(
                self.partial and self.revision and self.revision[0]
            )
            if (text, source, full_translation) != self.revision:
                self.preview_translation(
                    payload, source, full_translation, config, report
                )
                self.revision = text, source, full_translation
            self.partial = len(samples)
            return True

        if forced and config["voices"] is not None:
            turns = self.voice_turns(data[:end], config)
        groups = [(None, text)]
        if turns is not None:
            from live_voices import speaker_phrases

            spans = turns.result()
            if not text:
                recovered = recover_speech(engine, data[:end], config, spans)
                if recovered:
                    text, timed_words, source = recovered
                    payload["text"] = text
            groups = speaker_phrases(timed_words, spans, text)
        last_voiced = int(active[gaps[0]] if len(gaps) else active[-1])
        speech_end = self.start + min(end, (last_voiced + 1) * BLOCK)
        self.confirm(payload, groups, source, speech_end, config, report)
        self.start += end
        self.pcm = self.pcm[end:]
        self.partial = 0
        self.revision = None
        self.speculation = None
        return True


def listen(p, report):
    settings = p["settings"]
    threads = apply_limits(settings)
    import onnxruntime

    onnxruntime.disable_telemetry_events()
    from faster_whisper import WhisperModel
    import numpy as np
    from live_voices import LiveVoices

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
        duty=settings.get("gpu_duty", 100) if device == "cuda" else 100,
        glossary=french_glossary(settings),
    )
    config["mt_device"] = device
    config["mt_threads"] = min(threads or 2, 2) if device == "cpu" else threads
    config["voices"] = (
        LiveVoices(settings, p.get("profiles", []))
        if settings.get("diarization")
        else None
    )
    # Initialize native kernels before the first sample enters the capture.
    with gpu_timeslice(config["duty"]):
        warm, _ = engine.transcribe(
            np.zeros(RATE // 2, dtype="float32"),
            language=config["source"] or "en",
            beam_size=1,
            vad_filter=False,
            condition_on_previous_text=False,
        )
        list(warm)

    def warm_translation():
        translation_model(config["folder"], config["mt_threads"], config["mt_device"])
        Track.translate("OK", "en" if p["target"] != "en" else "fr", config)

    try:
        warm_translation()
    except (RuntimeError, ValueError, OSError) as error:
        message = str(error).lower()
        if (
            device != "cuda"
            or settings.get("strict_cuda")
            or not (
                cuda_unavailable(error)
                or "out of memory" in message
                or "requested int8_float16" in message
            )
        ):
            raise
        translation_model.cache_clear()
        config.update(mt_device="cpu", mt_threads=min(threads or 2, 2))
        report("Traduction sur CPU · transcription sur GPU")
        warm_translation()
    report("Écoute en direct", live={"ready": True})
    with (
        ThreadPoolExecutor(max_workers=1) as mt_pool,
        ThreadPoolExecutor(max_workers=1) as voice_pool,
    ):
        config.update(mt_pool=mt_pool, voice_pool=voice_pool)
        return run_tracks(root, p["tracks"], engine, config, report)


def run_tracks(root, count, engine, config, report):
    tracks = [Track(i, root) for i in range(count)]
    last = time.monotonic()
    while True:
        finished = (root / "finalise").exists()
        worked = [track.step(engine, config, finished, report) for track in tracks]
        if finished and not any(worked):
            return {}
        if not any(worked):
            time.sleep(0.025)
        if time.monotonic() - last > 15:
            report("Écoute en direct")
            last = time.monotonic()
