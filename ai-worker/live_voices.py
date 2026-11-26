"""Session-local speaker labels from short pyannote windows and clean voice samples."""

from itertools import combinations, groupby

import numpy as np

from voices import candidate, embedding, extractor, model_key

RATE = 16000
UNKNOWN = "uncertain"
OVERLAP = "overlap"


class SpeakerMemory:
    def __init__(self, profiles=(), match=0.54, margin=0.075):
        self.centres = []
        self.observations = []
        self.profiles = profiles
        self.votes = {}
        self.match = match
        self.margin = margin

    def assign(self, vectors, fresh=None):
        # Slots in one segmentation window denote distinct people. Reserve each
        # existing identity once, processing the strongest evidence first.
        scores = {
            slot: sorted(
                ((float(v @ c), i) for i, c in enumerate(self.centres)), reverse=True
            )
            for slot, v in vectors.items()
        }
        result = {}
        used = set()
        for slot in sorted(
            vectors, key=lambda s: scores[s][0][0] if scores[s] else -1, reverse=True
        ):
            ranks = scores[slot]
            vec = vectors[slot]
            if not ranks or ranks[0][0] < self.match - self.margin:
                if len(self.centres) >= 256:
                    continue
                identity = len(self.centres)
                self.centres.append(vec.copy())
                self.observations.append(1)
            else:
                best, identity = ranks[0]
                if (
                    best < self.match
                    or identity in used
                    or len(ranks) > 1
                    and best - ranks[1][0] < self.margin
                ):
                    continue
                self.observations[identity] += 1
                weight = 1 / min(self.observations[identity], 6)
                updated = (1 - weight) * self.centres[identity] + weight * vec
                self.centres[identity] = updated / np.linalg.norm(updated)
            used.add(identity)
            result[slot] = identity + 1
            if self.profiles and (fresh is None or slot in fresh):
                vote = candidate(
                    vec if fresh is None else fresh[slot],
                    self.profiles,
                    threshold=max(0.77, self.match),
                )
                previous, count = self.votes.get(identity + 1, (None, 0))
                profile = vote["profile"] if vote else None
                self.votes[identity + 1] = (
                    profile,
                    count + 1 if profile == previous else 1,
                )
        return result

    def suggestion(self, speaker):
        profile, count = self.votes.get(speaker, (None, 0))
        if profile is not None and count >= 2:
            return next((p["name"] for p in self.profiles if p["id"] == profile), None)
        return None


class LiveVoices:
    def __init__(self, settings, profiles):
        import onnxruntime as ort

        opts = ort.SessionOptions()
        opts.intra_op_num_threads = opts.inter_op_num_threads = 1
        self.model = ort.InferenceSession(
            settings["segmentation"], opts, providers=["CPUExecutionProvider"]
        )
        meta = self.model.get_modelmeta().custom_metadata_map
        self.size = int(meta.get("window_size", 0))
        self.stride = int(meta.get("receptive_field_shift", 0))
        self.field = int(meta.get("receptive_field_size", 0))
        slots = int(meta.get("num_speakers", 0))
        order = int(meta.get("powerset_max_classes", 0))
        if (
            int(meta.get("sample_rate", 0)) != RATE
            or self.size < RATE * 8
            or not 0 < self.stride <= self.field <= self.size
            or not 1 <= order <= slots <= 8
        ):
            raise ValueError("Modèle de segmentation incompatible avec le direct")
        self.classes = [
            c for n in range(order + 1) for c in combinations(range(slots), n)
        ]
        if len(self.classes) != int(meta.get("num_classes", 0)):
            raise ValueError("Modèle de segmentation incompatible avec le direct")
        self.input = self.model.get_inputs()[0].name
        self.encoder = extractor(settings["embedding"])
        key = model_key(settings["embedding"])
        compatible = (
            [p for p in profiles if p["model"] == key]
            if settings.get("recognize")
            else []
        )
        # WeSpeaker ResNet34 uses a different cosine scale from CAM++.
        wespeaker = (
            key == "5ef208a9da1453335308a6b6f4e6dfbd7e183a38b604de0a57664f45d257fe94"
        )
        self.memory = SpeakerMemory(
            compatible, 0.90 if wespeaker else 0.54, 0.035 if wespeaker else 0.075
        )

    def turns(self, audio, offset=0):
        length = len(audio)
        padded = np.zeros((1, 1, self.size), dtype=np.float32)
        padded[0, 0, :length] = audio
        logits = self.model.run(None, {self.input: padded})[0][0]
        codes = np.argmax(logits, axis=-1)
        # Assign every sample to its nearest output frame, including both edges.
        boundaries = np.clip(
            np.arange(len(codes) + 1) * self.stride + (self.field - self.stride) // 2,
            0,
            length,
        )
        boundaries[0] = 0
        edges = np.r_[0, np.flatnonzero(np.diff(codes)) + 1, len(codes)]
        spans = [
            (int(boundaries[a]), int(boundaries[b]), self.classes[codes[a]])
            for a, b in zip(edges, edges[1:])
            if boundaries[b] > boundaries[a]
        ]
        # Preserve short natural pauses and quiet consonants for the voice
        # encoder. Stop context at the nearest other talker or overlap.
        clean = {}
        durations = {}
        flank = int(RATE * 0.18)
        for i, (a, b, slots) in enumerate(spans):
            if len(slots) != 1:
                continue
            slot = slots[0]
            durations[slot] = durations.get(slot, 0) + max(0, b - max(a, offset))
            start = max(a - flank, spans[i - 1][0]) if i and not spans[i - 1][2] else a
            end = (
                min(b + flank, spans[i + 1][1])
                if i + 1 < len(spans) and not spans[i + 1][2]
                else b
            )
            ranges = clean.setdefault(slot, [])
            if ranges and ranges[-1][1] >= start:
                ranges[-1] = (ranges[-1][0], end)
            else:
                ranges.append((start, end))
        vectors, fresh = {}, {}
        for slot, ranges in clean.items():
            if durations[slot] < RATE:
                continue
            try:
                vectors[slot] = self.vector(audio, ranges, 0)
                if self.memory.profiles:
                    fresh[slot] = (
                        self.vector(audio, ranges, offset) if offset else vectors[slot]
                    )
            except ValueError:
                continue
        labels = self.memory.assign(vectors, fresh)
        return [
            (
                max(0, a - offset) / RATE,
                (b - offset) / RATE,
                OVERLAP if len(slots) > 1 else labels.get(slots[0], UNKNOWN),
            )
            for a, b, slots in spans
            if slots and b > offset
        ]

    def vector(self, audio, ranges, start):
        sample = np.concatenate(
            [audio[max(a, start) : b] for a, b in ranges if b > start]
        )
        pcm = (sample * 32768).clip(-32768, 32767).astype("<i2")
        return np.asarray(embedding(self.encoder, pcm.tobytes()))


def word_speaker(start, end, turns):
    durations = {}
    for a, b, speaker in turns:
        duration = min(end, b) - max(start, a)
        if duration > 0:
            durations[speaker] = durations.get(speaker, 0) + duration
    if not durations:
        return UNKNOWN
    if durations.get(OVERLAP, 0) >= 0.25 * max(0.001, end - start):
        return OVERLAP
    winner = max(durations, key=durations.get)
    return (
        winner
        if durations[winner] > max((end - start) * 0.3, sum(durations.values()) / 2)
        else UNKNOWN
    )


def speaker_phrases(words, turns, original):
    chunks = [
        (speaker, "".join(w[2] for w in group).strip())
        for speaker, group in groupby(words, lambda w: word_speaker(w[0], w[1], turns))
    ]
    if len(chunks) <= 1:
        return [(chunks[0][0] if chunks else UNKNOWN, original)]
    return [(speaker, text) for speaker, text in chunks if text]
