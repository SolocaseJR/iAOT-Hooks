#!/usr/bin/env python3
"""Fail fast when a Code Virtualizer ARM64/Mach-O input is unusable."""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path
from typing import Optional, Tuple


ARM64_CPU_TYPE = 0x0100000C
LC_SYMTAB = 0x2
N_EXT = 0x01
N_PEXT = 0x10
REGS_START = (0xA5A6A7A8, 0xA8A7A6A5, 0xB5A5B6A6, 0xB7A7A5A5)
REGS_END = (0xB8B7B6B5, 0xA6B6C6D6, 0xA7A6A4A4, 0xA4A5B1B2)
CODE_START = (0xA1A2A3A4, 0xA4A3A2A1, 0xB1A1B2A2, 0xB8A8A1A1)
CODE_END = (0xB6B5B3B6, 0xA2B2C2D2, 0xA9A8A2A2, 0xA0A9B9B8)


def die(message: str) -> None:
    print(f"!! Code Virtualizer input invalid: {message}", file=sys.stderr)
    raise SystemExit(1)


def check_sdk(path: Path) -> None:
    text = path.read_text(errors="ignore")
    arm_tokens = ("PLATFORM_ARM64", "__aarch64__", "__arm64__", "_M_ARM64")
    if not any(token in text for token in arm_tokens):
        die(
            f"{path} is the legacy x86/x64 SDK header. Use the matching ARM64 "
            "Include/C and StealthMode/C/StealthCodeRegsArea directories from "
            "the current Code Virtualizer installation."
        )
    print(f">> CV SDK check OK: ARM64 declarations found in {path}")


def words(data: bytes):
    usable = len(data) & ~3
    return struct.unpack(f"<{usable // 4}I", data[:usable])


def decode_mov_w_pair(first: int, second: int) -> Optional[int]:
    # MOVZ Wd,#low16 followed by MOVK Wd,#high16,LSL#16. Clang emits this
    # deterministic pair for the 32-bit CVSX sentinels in the -O0 stealth TU.
    if first & 0x7F800000 != 0x52800000:
        return None
    if second & 0x7F800000 != 0x72800000:
        return None
    if (first & 0x1F) != (second & 0x1F):
        return None
    if ((first >> 21) & 0x3) != 0 or ((second >> 21) & 0x3) != 1:
        return None
    return ((second >> 5) & 0xFFFF) << 16 | ((first >> 5) & 0xFFFF)


def contains_code_sentinels(data: bytes, expected: Tuple[int, ...]) -> bool:
    decoded = []
    ws = words(data)
    for i in range(len(ws) - 1):
        value = decode_mov_w_pair(ws[i], ws[i + 1])
        if value is not None:
            decoded.append(value)
    for i in range(len(decoded) - len(expected) + 1):
        if tuple(decoded[i : i + len(expected)]) == expected:
            return True
    return False


def check_binary(path: Path) -> None:
    data = path.read_bytes()
    if len(data) < 32:
        die(f"{path} is too small")
    magic, cpu_type = struct.unpack_from("<II", data)
    if magic != 0xFEEDFACF:
        die(f"{path} is not a little-endian Mach-O 64 image")
    if cpu_type != ARM64_CPU_TYPE:
        die(f"{path} is not ARM64 (cpu type 0x{cpu_type:08x})")

    for name, sentinels in (("register start", REGS_START), ("register end", REGS_END)):
        pattern = struct.pack("<4I", *sentinels)
        if data.count(pattern) != 1:
            die(f"expected exactly one {name} sentinel, found {data.count(pattern)}")
    if not contains_code_sentinels(data, CODE_START):
        die("ARM64 code stealth start sentinel was optimized/stripped out")
    if not contains_code_sentinels(data, CODE_END):
        die("ARM64 code stealth end sentinel was optimized/stripped out")
    print(f">> CV Mach-O check OK: ARM64 code + register stealth areas found in {path}")


def macho_symbol_names(path: Path) -> list[str]:
    data = path.read_bytes()
    if len(data) < 32:
        die(f"{path} is too small")
    magic, cpu_type, _, _, ncmds, _, _, _ = struct.unpack_from("<IiiIIIII", data)
    if magic != 0xFEEDFACF or cpu_type != ARM64_CPU_TYPE:
        die(f"{path} is not an ARM64 Mach-O 64 image")

    offset = 32
    symtab = None
    for _ in range(ncmds):
        if offset + 8 > len(data):
            die("truncated Mach-O load command")
        command, command_size = struct.unpack_from("<II", data, offset)
        if command_size < 8 or offset + command_size > len(data):
            die("invalid Mach-O load-command size")
        if command == LC_SYMTAB:
            if command_size < 24:
                die("truncated LC_SYMTAB command")
            symtab = struct.unpack_from("<IIII", data, offset + 8)
            break
        offset += command_size
    if symtab is None:
        return []

    symoff, nsyms, stroff, strsize = symtab
    if symoff + nsyms * 16 > len(data) or stroff + strsize > len(data):
        die("LC_SYMTAB points outside the file")
    strings = data[stroff : stroff + strsize]
    names: list[str] = []
    for index in range(nsyms):
        string_index, _, _, _, _ = struct.unpack_from("<IBBHQ", data, symoff + index * 16)
        if string_index >= len(strings):
            die("symbol has an invalid string-table index")
        end = strings.find(b"\0", string_index)
        if end < 0:
            die("unterminated symbol name")
        names.append(strings[string_index:end].decode("utf-8", errors="replace"))
    return names


def check_final_symbols(path: Path) -> None:
    # The dynamic loader still needs imported system/framework names. This checks
    # the static symbol table where IDA gets Pulsar function names.
    leaked = [name for name in macho_symbol_names(path) if "Pulsar" in name]
    if leaked:
        preview = ", ".join(leaked[:8])
        suffix = " ..." if len(leaked) > 8 else ""
        die(f"Pulsar symbols remain in LC_SYMTAB ({preview}{suffix})")
    print(f">> symbol hygiene OK: no Pulsar names remain in {path}'s LC_SYMTAB")


def scrub_final_symbols(path: Path) -> None:
    """Clear local/private-extern names left behind by a Mach-O strip pass.

    dyld resolves imports from its bind/export information, not the local symbol
    table. We never clear public external names here: a remaining Pulsar public
    export is a build-configuration error and check_final_symbols reports it.
    """
    data = bytearray(path.read_bytes())
    if len(data) < 32:
        die(f"{path} is too small")
    magic, cpu_type, _, _, ncmds, _, _, _ = struct.unpack_from("<IiiIIIII", data)
    if magic != 0xFEEDFACF or cpu_type != ARM64_CPU_TYPE:
        die(f"{path} is not an ARM64 Mach-O 64 image")

    offset = 32
    symoff = nsyms = stroff = strsize = None
    for _ in range(ncmds):
        if offset + 8 > len(data):
            die("truncated Mach-O load command")
        command, command_size = struct.unpack_from("<II", data, offset)
        if command_size < 8 or offset + command_size > len(data):
            die("invalid Mach-O load-command size")
        if command == LC_SYMTAB:
            if command_size < 24:
                die("truncated LC_SYMTAB command")
            symoff, nsyms, stroff, strsize = struct.unpack_from("<IIII", data, offset + 8)
            break
        offset += command_size
    if symoff is None:
        die("Mach-O has no LC_SYMTAB")
    if symoff + nsyms * 16 > len(data) or stroff + strsize > len(data):
        die("LC_SYMTAB points outside the file")

    strings = data[stroff : stroff + strsize]
    cleared = 0
    for index in range(nsyms):
        entry = symoff + index * 16
        string_index, n_type, _, _, _ = struct.unpack_from("<IBBHQ", data, entry)
        if string_index >= len(strings):
            die("symbol has an invalid string-table index")
        end = strings.find(b"\0", string_index)
        if end < 0:
            die("unterminated symbol name")
        is_local_or_private = not (n_type & N_EXT) or bool(n_type & N_PEXT)
        if is_local_or_private and string_index:
            struct.pack_into("<I", data, entry, 0)
            cleared += 1

    path.write_bytes(data)
    print(f">> cleared {cleared} local/private Mach-O symbol names from {path}")


def main() -> None:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--sdk", type=Path)
    group.add_argument("--binary", type=Path)
    group.add_argument("--final-symbols", type=Path)
    group.add_argument("--scrub-final-symbols", type=Path)
    args = parser.parse_args()
    if args.sdk:
        check_sdk(args.sdk)
    elif args.binary:
        check_binary(args.binary)
    elif args.final_symbols:
        check_final_symbols(args.final_symbols)
    else:
        scrub_final_symbols(args.scrub_final_symbols)


if __name__ == "__main__":
    main()
