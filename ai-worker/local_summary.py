"""Local GGUF decoding for a single, bounded summary batch."""

from contextlib import closing
from itertools import groupby
import json
import re
import time


def compact_repetitions(text):
    # Preserve the count for the model. The recording and its transcript stay intact.
    parts = re.split(r"(?<=[.!?。！？])\s+|(?<=[。！？])(?=\S)|\n+", text.strip())
    if len(parts) < 3:
        return text
    compressed = []
    changed = False
    for sentence, group in groupby(parts):
        count = sum(1 for _ in group)
        if count >= 3:
            compressed.append(f"{sentence} [×{count}]")
            changed = True
        else:
            compressed.extend([sentence] * count)
    return " ".join(compressed) if changed else text


def decode(llm, options, stage, progress):
    """Return only a completed JSON response. Stop a run of duplicate facts early."""
    content = ""
    offset = None
    decoder = json.JSONDecoder()
    seen = set()
    duplicates = 0
    last_new_fact = last_update = time.monotonic()
    with closing(llm.create_chat_completion(**options, stream=True)) as stream:
        for event in stream:
            choice = event["choices"][0]
            fragment = choice["delta"].get("content") or ""
            content += fragment
            if choice["finish_reason"] is not None:
                return content if choice["finish_reason"] == "stop" else None

            now = time.monotonic()
            if now - last_new_fact >= 120:
                return None
            if fragment and now - last_update >= 2:
                progress(stage, phase="summary_decode", generated_chars=len(content))
                last_update = now
            if "}" not in fragment:
                continue
            if offset is None:
                start = re.match(r'\s*\{\s*"faits"\s*:\s*\[', content)
                if start is None:
                    continue
                offset = start.end()
            # Decode each closed fact once, across arbitrary stream chunk boundaries.
            while offset < len(content):
                if content[offset] in ", \r\n\t":
                    offset += 1
                    continue
                if content[offset] != "{":
                    break
                try:
                    fact, offset = decoder.raw_decode(content, offset)
                except json.JSONDecodeError:
                    break
                key = json.dumps(fact, sort_keys=True, ensure_ascii=False)
                duplicates = duplicates + 1 if key in seen else 0
                if duplicates >= 3:
                    return None
                if key not in seen:
                    last_new_fact = now
                    seen.add(key)
    return None
