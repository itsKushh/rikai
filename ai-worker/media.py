"""Inspect local media and stream its first audio track into Rikai's capture format."""
import json
from pathlib import Path
import wave

import av


def probe(params, progress):
    source = Path(params["path"])
    with av.open(str(source), options={"protocol_whitelist": "file"}) as media:
        if not media.streams.audio:
            raise ValueError("Ce fichier ne contient aucune piste audio.")
        video = next((s for s in media.streams.video if not (s.disposition & av.stream.Disposition.attached_pic)), None)
        info = {"size": source.stat().st_size, "duration_ms": (media.duration or 0) // 1000,
                "video": video is not None, "format": media.format.name}
        if video:
            info.update(width=video.width, height=video.height)
            try:
                frame = next(media.decode(video))
                width = min(frame.width, 480)
                frame = frame.reformat(width=width, height=max(1, round(width * frame.height / frame.width)), format="rgb24")
                encoder = av.CodecContext.create("png", "w")
                encoder.width, encoder.height, encoder.pix_fmt = frame.width, frame.height, "rgb24"
                encoder.time_base = video.time_base
                Path(params["thumbnail"]).write_bytes(bytes(encoder.encode(frame)[0]))
            except (StopIteration, av.FFmpegError, OSError, ValueError, IndexError):
                pass
        return info


def import_media(params, progress):
    source, dest = Path(params["path"]), Path(params["directory"])
    track = "piste-1.wav"
    count = 0
    with av.open(str(source), options={"protocol_whitelist": "file"}) as media, wave.open(str(dest / track), "wb") as out:
        if not media.streams.audio:
            raise ValueError("Ce fichier ne contient aucune piste audio.")
        out.setparams((1, 2, 16000, 0, "NONE", ""))
        resampler = av.AudioResampler(format="s16", layout="mono", rate=16000)
        def write(frames):
            nonlocal count
            for frame in frames:
                data = bytes(frame.planes[0])[:frame.samples * 2]
                out.writeframesraw(data)
                count += frame.samples
        for frame in media.decode(media.streams.audio[0]):
            frame.pts = None
            before = count
            write(resampler.resample(frame))
            if count // 160000 != before // 160000:
                progress(f"Import audio · {count // 16000} s")
        write(resampler.resample(None))
    if not count:
        raise ValueError("La piste audio est vide.")
    duration = max(1, count * 1000 // 16000)
    identity = {"kind": "Media"}
    tracks = [{"file": track, "input": {"name": source.name, "source": identity, "peak": 0.0}}]
    (dest / "sources.json").write_text(json.dumps(dict(version=1, rate=16000, tracks=tracks)), encoding="utf-8")
    (dest / "finalise").write_text(str(duration), encoding="utf-8")
    return {"duration_ms": duration, "file": track}
