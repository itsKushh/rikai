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
    """Read a completed Markdown or JSON response, stopping stalled or cyclic output."""
    content = ""
    markdown = "response_format" not in options
    pending_line = ""
    starts = []
    quoted = escaped = False
    seen = set()
    duplicates = 0
    last_new_fact = None
    last_update = time.monotonic()
    with closing(llm.create_chat_completion(**options, stream=True)) as stream:
        for event in stream:
            choice = event["choices"][0]
            fragment = choice["delta"].get("content") or ""
            start = len(content)
            content += fragment
            if choice["finish_reason"] is not None:
                return content if choice["finish_reason"] == "stop" else None

            now = time.monotonic()
            if last_new_fact is None and fragment:
                last_new_fact = now
            if last_new_fact is not None and now - last_new_fact >= 120:
                return None
            if fragment and now - last_update >= 2:
                progress(stage, phase="summary_decode", generated_chars=len(content))
                last_update = now
            if markdown:
                lines = (pending_line + fragment).split("\n")
                pending_line = lines.pop()
                for line in lines:
                    line = line.strip()
                    if len(line) < 12:
                        continue
                    if line in seen:
                        duplicates += 1
                        if duplicates >= 3:
                            return None
                    else:
                        seen.add(line)
                        duplicates = 0
                        last_new_fact = now
                continue
            for offset, char in enumerate(fragment, start):
                if quoted:
                    if escaped:
                        escaped = False
                    elif char == "\\":
                        escaped = True
                    elif char == '"':
                        quoted = False
                elif char == '"':
                    quoted = True
                elif char == "{":
                    starts.append(offset)
                elif char == "}" and starts:
                    begin = starts.pop()
                    if len(starts) != 1:
                        continue
                    try:
                        fact = json.loads(content[begin : offset + 1])
                    except ValueError:
                        return None
                    key = json.dumps(fact, sort_keys=True, ensure_ascii=False)
                    duplicates = duplicates + 1 if key in seen else 0
                    if duplicates >= 3:
                        return None
                    if key not in seen:
                        last_new_fact = now
                        seen.add(key)
    return None
