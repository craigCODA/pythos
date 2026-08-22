#!/usr/bin/env python
"""Build deterministic Phase 13 package-format fixtures."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


PACKAGE_ARTIFACT_MAGIC = b"PYTHPKG0"
PACKAGE_ARTIFACT_HEADER_LEN = 160
PACKAGE_MANIFEST_MAGIC = b"PYTHMAN0"


def build_empty_manifest() -> bytes:
    return PACKAGE_MANIFEST_MAGIC + (0).to_bytes(4, "little")


def artifact_digest_domain(artifact: bytes) -> bytes:
    if len(artifact) < PACKAGE_ARTIFACT_HEADER_LEN:
        raise ValueError("artifact is shorter than the Phase 13 header")
    out = bytearray(artifact)
    out[96:128] = bytes(32)
    return bytes(out)


def build_format_fixture() -> bytes:
    manifest = build_empty_manifest()
    content_table = b""
    content = b""
    manifest_offset = PACKAGE_ARTIFACT_HEADER_LEN
    content_table_offset = manifest_offset + len(manifest)
    content_offset = content_table_offset + len(content_table)

    header = bytearray(PACKAGE_ARTIFACT_HEADER_LEN)
    header[0:8] = PACKAGE_ARTIFACT_MAGIC
    header[8:10] = (0).to_bytes(2, "little")
    header[10:12] = (1).to_bytes(2, "little")
    header[12:16] = PACKAGE_ARTIFACT_HEADER_LEN.to_bytes(4, "little")
    header[16:24] = manifest_offset.to_bytes(8, "little")
    header[24:32] = len(manifest).to_bytes(8, "little")
    header[32:40] = content_table_offset.to_bytes(8, "little")
    header[40:48] = len(content_table).to_bytes(8, "little")
    header[48:56] = content_offset.to_bytes(8, "little")
    header[56:64] = len(content).to_bytes(8, "little")
    header[64:96] = hashlib.sha256(manifest).digest()

    artifact = bytes(header) + manifest + content_table + content
    digest = hashlib.sha256(artifact_digest_domain(artifact)).digest()
    artifact = artifact[:96] + digest + artifact[128:]
    return artifact


def self_test() -> None:
    artifact = build_format_fixture()
    if artifact[:8] != PACKAGE_ARTIFACT_MAGIC:
        raise AssertionError("bad fixture magic")
    if int.from_bytes(artifact[12:16], "little") != PACKAGE_ARTIFACT_HEADER_LEN:
        raise AssertionError("bad fixture header length")
    manifest_offset = int.from_bytes(artifact[16:24], "little")
    manifest_len = int.from_bytes(artifact[24:32], "little")
    manifest = artifact[manifest_offset : manifest_offset + manifest_len]
    if manifest != build_empty_manifest():
        raise AssertionError("bad fixture manifest")
    if artifact[64:96] != hashlib.sha256(manifest).digest():
        raise AssertionError("bad fixture manifest digest")
    if artifact[96:128] != hashlib.sha256(artifact_digest_domain(artifact)).digest():
        raise AssertionError("bad fixture artifact digest")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        print("PHASE13_PACKAGE_FIXTURE_OK")
        return 0
    if args.output is None:
        raise SystemExit("--output is required unless --self-test is used")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(build_format_fixture())
    print(f"PHASE13_PACKAGE_FIXTURE_READY {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
