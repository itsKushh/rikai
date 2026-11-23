import os
import sys
from pathlib import Path
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from resources import apply_limits, paced_segments


class Resources(unittest.TestCase):
    def test_thread_limit_and_default(self):
        with patch.dict(os.environ), patch("resources.os.cpu_count", return_value=8):
            self.assertEqual(apply_limits({}), 0)
            self.assertEqual(apply_limits({"cpu_threads": 20}), 8)
            self.assertEqual(os.environ["OMP_NUM_THREADS"], "8")

    def test_gpu_duty_reserves_time_between_inferences(self):
        with (
            patch("resources.time.monotonic", side_effect=[1, 2, 4, 5, 8, 8]),
            patch("resources.time.sleep") as sleep,
        ):
            self.assertEqual(list(paced_segments([1, 2], 50)), [1, 2])
            self.assertEqual([c.args for c in sleep.call_args_list], [(1,), (1,)])
        with patch("resources.time.sleep") as sleep:
            self.assertEqual(list(paced_segments([1], 100)), [1])
            sleep.assert_not_called()

    @unittest.skipUnless(sys.platform == "win32", "Windows Job Object")
    def test_memory_limit_in_child(self):
        import subprocess

        check = """
import ctypes,sys
from ctypes import wintypes
import resources
resources.apply_limits({"ram_gib": 3})
k=ctypes.WinDLL("kernel32",use_last_error=True)
buf=ctypes.create_string_buffer(144)
job=wintypes.HANDLE(resources._job)
member=wintypes.BOOL()
assert k.IsProcessInJob(wintypes.HANDLE(-1),job,ctypes.byref(member)) and member.value
assert k.QueryInformationJobObject(job,9,ctypes.byref(buf),len(buf),None)
assert int.from_bytes(buf.raw[112:120],"little")==3*1024**3
"""
        subprocess.run(
            [sys.executable, "-c", check],
            cwd=Path(__file__).resolve().parents[1],
            check=True,
        )
