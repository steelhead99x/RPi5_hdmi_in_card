#!/usr/bin/env python3
"""Build the single-mode HDMI EDID files used by enablehdmi.sh.

Each file asks the source for one CEA-861 picture, lists that picture as the
preferred timing, and includes a short HDMI audio block. The capture script
loads the text with v4l2-ctl format=hex.
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

# name, CEA VIC, width, height, refresh Hz, pixel clock Hz,
# h blank, h front, h sync, v blank, v front, v sync
MODES = (
    ("720p60edid", 4, 1280, 720, 60, 74_250_000, 370, 110, 40, 30, 5, 5),
    ("1080p25edid", 33, 1920, 1080, 25, 74_250_000, 720, 528, 44, 45, 4, 5),
    ("1080p30edid", 34, 1920, 1080, 30, 74_250_000, 280, 88, 44, 45, 4, 5),
    ("1080p50edid", 31, 1920, 1080, 50, 148_500_000, 720, 528, 44, 45, 4, 5),
    ("1080p60edid", 16, 1920, 1080, 60, 148_500_000, 280, 88, 44, 45, 4, 5),
)

# Rec.709-style chromaticity, copied from a block edid-decode already accepts.
CHROMA = bytes.fromhex("ee91a3544c99260f5054")
PRODUCT_NAME = b"HDMI IN\n"


def manufacturer_id(code: str) -> bytes:
    if len(code) != 3 or not code.isalpha() or not code.isupper():
        raise ValueError(f"manufacturer code must be three letters: {code}")
    value = 0
    for char in code:
        value = (value << 5) | (ord(char) - ord("A") + 1)
    return bytes(((value >> 8) & 0xFF, value & 0xFF))


def checksum_block(block: bytes) -> bytes:
    if len(block) != 128:
        raise ValueError(f"EDID block must be 128 bytes, got {len(block)}")
    body = bytearray(block[:127])
    body.append((-sum(body)) & 0xFF)
    return bytes(body)


def detailed_timing(
    width: int,
    height: int,
    pixel_clock_hz: int,
    h_blank: int,
    h_front: int,
    h_sync: int,
    v_blank: int,
    v_front: int,
    v_sync: int,
) -> bytes:
    if pixel_clock_hz % 10_000 != 0:
        raise ValueError("pixel clock must be a multiple of 10 kHz")
    h_back = h_blank - h_front - h_sync
    v_back = v_blank - v_front - v_sync
    if min(h_back, v_back) <= 0:
        raise ValueError("blanking is smaller than the porches")
    clock = pixel_clock_hz // 10_000
    raw = bytearray(18)
    raw[0] = clock & 0xFF
    raw[1] = (clock >> 8) & 0xFF
    raw[2] = width & 0xFF
    raw[3] = h_blank & 0xFF
    raw[4] = ((width >> 4) & 0xF0) | ((h_blank >> 8) & 0x0F)
    raw[5] = height & 0xFF
    raw[6] = v_blank & 0xFF
    raw[7] = ((height >> 4) & 0xF0) | ((v_blank >> 8) & 0x0F)
    raw[8] = h_front & 0xFF
    raw[9] = h_sync & 0xFF
    raw[10] = ((v_front & 0x0F) << 4) | (v_sync & 0x0F)
    raw[11] = (
        (((h_front >> 8) & 0x03) << 6)
        | (((h_sync >> 8) & 0x03) << 4)
        | (((v_front >> 4) & 0x03) << 2)
        | ((v_sync >> 4) & 0x03)
    )
    # Digital separate sync, positive horizontal and vertical polarity.
    raw[17] = 0x1E
    return bytes(raw)


def descriptor(tag: int, payload: bytes) -> bytes:
    if len(payload) != 13:
        raise ValueError(f"descriptor payload must be 13 bytes, got {len(payload)}")
    return bytes((0x00, 0x00, 0x00, tag, 0x00)) + payload


def monitor_name() -> bytes:
    name = PRODUCT_NAME
    if len(name) > 13:
        raise ValueError("monitor name is longer than 13 bytes")
    return descriptor(0xFC, name + b" " * (13 - len(name)))


def range_limits(width: int, height: int, refresh: int, h_blank: int, v_blank: int, pixel_clock_hz: int) -> bytes:
    h_total = width + h_blank
    v_total = height + v_blank
    h_khz = (refresh * v_total) / 1000
    h_min = math.floor(h_khz)
    h_max = math.ceil(h_khz)
    if h_min < 1 or h_max > 255 or refresh < 1 or refresh > 255:
        raise ValueError("timing does not fit in a range descriptor")
    clock_10mhz = math.ceil((pixel_clock_hz / 1_000_000) / 10)
    if clock_10mhz > 255:
        raise ValueError("pixel clock does not fit in a range descriptor")
    payload = bytes(
        (
            refresh,
            refresh,
            h_min,
            h_max,
            clock_10mhz,
            0x00,
            0x0A,
            0x20,
            0x20,
            0x20,
            0x20,
            0x20,
            0x20,
        )
    )
    return descriptor(0xFD, payload)


def base_block(mode: tuple) -> bytes:
    _name, _vic, width, height, refresh, pixel_clock, h_blank, h_front, h_sync, v_blank, v_front, v_sync = mode
    block = bytearray(128)
    block[0:8] = bytes.fromhex("00ffffffffffff00")
    block[8:10] = manufacturer_id("HDM")
    block[10:12] = (1).to_bytes(2, "little")
    block[16] = 1
    block[17] = 2026 - 1990
    block[18] = 1
    block[19] = 3
    block[20] = 0x80
    block[23] = 0x78
    block[24] = 0x0A
    block[25:35] = CHROMA
    for index in range(8):
        block[38 + index * 2] = 0x01
        block[39 + index * 2] = 0x01
    timing = detailed_timing(
        width, height, pixel_clock, h_blank, h_front, h_sync, v_blank, v_front, v_sync
    )
    block[54:72] = timing
    block[72:90] = monitor_name()
    block[90:108] = range_limits(width, height, refresh, h_blank, v_blank, pixel_clock)
    block[108:126] = descriptor(0x10, b"\x00" * 13)
    block[126] = 1
    return checksum_block(block)


def extension_block(mode: tuple) -> bytes:
    _name, vic, width, height, _refresh, pixel_clock, h_blank, h_front, h_sync, v_blank, v_front, v_sync = mode
    block = bytearray(128)
    block[0] = 0x02
    block[1] = 0x03
    # Basic audio, one native detailed timing, RGB only.
    block[3] = 0x41
    data = bytearray()
    # Tag 2, one VIC byte. The high bit marks that VIC as the native mode.
    data += bytes((0x41, 0x80 | vic))
    data += bytes((0x23, 0x09, 0x07, 0x07))
    data += bytes((0x83, 0x01, 0x00, 0x00))
    # HDMI vendor block, physical address 1.0.0.0.
    data += bytes((0x65, 0x03, 0x0C, 0x00, 0x10, 0x00))
    if 4 + len(data) + 18 > 127:
        raise ValueError("CEA extension does not fit")
    block[2] = 4 + len(data)
    block[4 : 4 + len(data)] = data
    timing = detailed_timing(
        width, height, pixel_clock, h_blank, h_front, h_sync, v_blank, v_front, v_sync
    )
    start = block[2]
    block[start : start + 18] = timing
    return checksum_block(block)


def build_edid(mode: tuple) -> bytes:
    return base_block(mode) + extension_block(mode)


def to_hex(edid: bytes) -> str:
    lines = []
    for offset in range(0, len(edid), 16):
        lines.append(" ".join(f"{byte:02x}" for byte in edid[offset : offset + 16]))
    return "\n".join(lines) + "\n"


def mode_by_name(name: str) -> tuple:
    for mode in MODES:
        if mode[0] == name:
            return mode
    raise KeyError(name)


def write_files(directory: Path) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    for mode in MODES:
        path = directory / mode[0]
        path.write_text(to_hex(build_edid(mode)), encoding="ascii")


def check_files(directory: Path) -> list[str]:
    problems = []
    for mode in MODES:
        path = directory / mode[0]
        expected = to_hex(build_edid(mode))
        if not path.is_file():
            problems.append(f"missing {path}")
            continue
        actual = path.read_text(encoding="ascii")
        if actual != expected:
            problems.append(f"out of date {path}")
    return problems


def main(argv: list[str]) -> int:
    directory = Path(__file__).resolve().parent
    if argv[1:] == ["--check"]:
        problems = check_files(directory)
        if problems:
            print("\n".join(problems), file=sys.stderr)
            print("Run python3 hdmi2csi2card/build_edid.py to regenerate the files.", file=sys.stderr)
            return 1
        return 0
    if argv[1:]:
        print("Usage: build_edid.py [--check]", file=sys.stderr)
        return 2
    write_files(directory)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
