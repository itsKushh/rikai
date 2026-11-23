"""Limites du seul processus de transcription, en unités affichées par l’interface."""

from contextlib import contextmanager
import ctypes
from ctypes import wintypes
import os
import sys
import time

_job = None


def apply_limits(settings):
    threads = min(os.cpu_count() or 1, max(0, int(settings.get("cpu_threads", 0))))
    if threads:
        for name in ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS"):
            os.environ[name] = str(threads)
    gib = int(settings.get("ram_gib", 0))
    if gib > 0 and sys.platform == "win32":
        limit_process_memory(gib * 1024**3)
    return threads


def limit_process_memory(limit):
    global _job

    # JOBOBJECT_EXTENDED_LIMIT_INFORMATION (Windows SDK).
    class Basic(ctypes.Structure):
        _fields_ = [
            ("process_time", ctypes.c_int64),
            ("job_time", ctypes.c_int64),
            ("flags", wintypes.DWORD),
            ("min_ws", ctypes.c_size_t),
            ("max_ws", ctypes.c_size_t),
            ("processes", wintypes.DWORD),
            ("affinity", ctypes.c_size_t),
            ("priority", wintypes.DWORD),
            ("scheduling", wintypes.DWORD),
        ]

    class Extended(ctypes.Structure):
        _fields_ = [
            ("basic", Basic),
            ("io", ctypes.c_uint64 * 6),
            ("process_limit", ctypes.c_size_t),
            ("job_limit", ctypes.c_size_t),
            ("process_peak", ctypes.c_size_t),
            ("job_peak", ctypes.c_size_t),
        ]

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    create = kernel.CreateJobObjectW
    create.argtypes = (ctypes.c_void_p, wintypes.LPCWSTR)
    create.restype = wintypes.HANDLE
    kernel.SetInformationJobObject.argtypes = (
        wintypes.HANDLE,
        ctypes.c_int,
        ctypes.c_void_p,
        wintypes.DWORD,
    )
    kernel.AssignProcessToJobObject.argtypes = (wintypes.HANDLE, wintypes.HANDLE)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
    job = create(None, None)
    limits = Extended()
    limits.basic.flags = 0x100  # JOB_OBJECT_LIMIT_PROCESS_MEMORY
    limits.process_limit = limit
    if (
        not job
        or not kernel.SetInformationJobObject(
            job, 9, ctypes.byref(limits), ctypes.sizeof(limits)
        )
        or not kernel.AssignProcessToJobObject(job, kernel.GetCurrentProcess())
    ):
        err = ctypes.get_last_error()
        if job:
            kernel.CloseHandle(job)
        raise OSError(
            err, "Windows n’a pas pu appliquer la limite de mémoire du worker"
        )
    _job = job  # Garder le job ouvert jusqu’à la fin de ce worker.


@contextmanager
def gpu_timeslice(duty):
    if duty >= 100:
        yield
        return
    started = time.monotonic()
    try:
        yield
    finally:
        elapsed = time.monotonic() - started
        if elapsed >= 0.001:
            time.sleep(elapsed * (100 / max(10, duty) - 1))


def paced_segments(segments, duty):
    if duty >= 100:
        yield from segments
        return
    iterator = iter(segments)
    while True:
        with gpu_timeslice(duty):
            try:
                segment = next(iterator)
            except StopIteration:
                return
        yield segment
