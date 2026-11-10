#!/usr/bin/env python3
"""Check the shipped EDID files against the mode each name claims."""

from __future__ import annotations

import importlib.util
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARD = ROOT / "hdmi2csi2card"
BUILDER = CARD / "build_edid.py"


def load_builder():
    spec = importlib.util.spec_from_file_location("build_edid", BUILDER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def parse_hex(text: str) -> bytes:
    if "\r" in text:
        raise AssertionError("EDID file contains CR characters")
    digits = []
    for char in text:
        if char in " \n\t":
            continue
        if char not in "0123456789abcdefABCDEF":
            raise AssertionError(f"unexpected character {char!r}")
        digits.append(char)
    raw = "".join(digits)
    if len(raw) % 2:
        raise AssertionError("odd number of hex digits")
    return bytes(int(raw[index : index + 2], 16) for index in range(0, len(raw), 2))


def timing(block: bytes) -> tuple[int, int, float]:
    clock = (block[1] << 8 | block[0]) * 10_000
    if clock == 0:
        raise AssertionError("expected a detailed timing")
    width = block[2] | ((block[4] & 0xF0) << 4)
    h_blank = block[3] | ((block[4] & 0x0F) << 8)
    height = block[5] | ((block[7] & 0xF0) << 4)
    v_blank = block[6] | ((block[7] & 0x0F) << 8)
    hertz = clock / ((width + h_blank) * (height + v_blank))
    return width, height, hertz


def cea_blocks(extension: bytes) -> list[tuple[int, bytes]]:
    end = extension[2]
    index = 4
    blocks = []
    while index < end:
        header = extension[index]
        length = header & 0x1F
        tag = header >> 5
        payload = extension[index + 1 : index + 1 + length]
        if len(payload) != length:
            raise AssertionError("CEA block runs past the data-block end")
        blocks.append((tag, payload))
        index += 1 + length
    if index != end:
        raise AssertionError("CEA parser did not stop on the detailed-timing offset")
    return blocks


class EdidTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.builder = load_builder()

    def test_files_match_generator(self):
        result = subprocess.run(
            ["python3", str(BUILDER), "--check"],
            check=False,
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_each_mode(self):
        for mode in self.builder.MODES:
            name, vic, width, height, refresh = mode[:5]
            with self.subTest(name=name):
                text = (CARD / name).read_text(encoding="ascii")
                data = parse_hex(text)
                self.assertEqual(data, self.builder.build_edid(mode))
                self.assertEqual(len(data), 256)
                self.assertEqual(data[:8], bytes.fromhex("00ffffffffffff00"))
                self.assertEqual(sum(data[:128]) & 0xFF, 0)
                self.assertEqual(sum(data[128:]) & 0xFF, 0)
                self.assertEqual(data[126], 1)
                got = timing(data[54:72])
                self.assertEqual(got[0], width)
                self.assertEqual(got[1], height)
                self.assertAlmostEqual(got[2], refresh, places=2)
                self.assertEqual(timing(data[data[130] + 128 : data[130] + 128 + 18])[:2], (width, height))
                self.assertEqual(data[72:90][5:12], b"HDMI IN")
                h_khz = refresh * (height + mode[9]) / 1000
                self.assertLessEqual(data[97], h_khz)
                self.assertGreaterEqual(data[98], h_khz)
                self.assertLessEqual(data[95], refresh)
                self.assertGreaterEqual(data[96], refresh)
                flags = data[131]
                self.assertEqual(flags & 0x30, 0, "EDID must not ask for YCbCr")
                self.assertEqual(flags & 0x40, 0x40)
                blocks = cea_blocks(data[128:])
                video = [payload for tag, payload in blocks if tag == 2]
                self.assertEqual(video, [bytes((0x80 | vic,))])
                vendor = [payload for tag, payload in blocks if tag == 3]
                self.assertEqual(len(vendor), 1)
                self.assertEqual(vendor[0][:3], bytes.fromhex("030c00"))
                decoded = subprocess.run(
                    ["edid-decode", str(CARD / name)],
                    check=True,
                    text=True,
                    capture_output=True,
                )
                output = decoded.stdout
                self.assertNotIn("Supports YCbCr", output)
                self.assertEqual(output.count("\n    VIC"), 1)
                self.assertIn("Vendor-Specific Data Block (HDMI)", output)
                self.assertIn(f"{width}x{height}", output.split("DTD 1:")[1].splitlines()[0])


if __name__ == "__main__":
    unittest.main()
