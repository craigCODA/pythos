#!/usr/bin/env python
"""Build deterministic Phase 13 package-format fixtures."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "phase13-package-fixture"
INDEPENDENT_SEED_SOURCE = ROOT / "programs" / "phase13" / "create-seed.pyth"
INDEPENDENT_SEED_SCHEMA = ROOT / "programs" / "phase13" / "schemas" / "seed.v0.schema"
INDEPENDENT_SEED_GRAPH = TARGET / "independent-seed.tig"
PACKAGE_ARTIFACT_MAGIC = b"PYTHPKG0"
PACKAGE_ARTIFACT_HEADER_LEN = 160
PACKAGE_MANIFEST_MAGIC = b"PYTHMAN0"
CONTENT_ENTRY_V0_LEN = 64
MANIFEST_RECORD_SCHEMA_DECLARATION = 1
MANIFEST_RECORD_PACKAGE_EXPORT = 2
MANIFEST_RECORD_CAPABILITY_REQUIREMENT = 3
PACKAGE_EXPORT_KIND_TOOL = 1
PACKAGE_CONTENT_ROLE_SCHEMA_DESCRIPTOR = 2
PACKAGE_CONTENT_ROLE_PYTHTIG_GRAPH = 2
PACKAGE_CONTENT_FORMAT_BYTES = 1
PACKAGE_LAUNCH_REQUIREMENT_ID = 7
PACKAGE_LAUNCH_IMPORT_SLOT = 0
PYTHTIG_RESOURCE_OBJECT_WORKSPACE = 2
PYTHTIG_RIGHTS_CREATE = 0x0008
FORBIDDEN_AUTHORITY_FIELDS = (
    b"app",
    b"desktop",
    b"launcher",
    b"window",
    b"filesystem",
)


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


def push_manifest_record(
    out: bytearray, record_type: int, name: bytes, payload: bytes
) -> None:
    out.extend(record_type.to_bytes(2, "little"))
    out.extend((0).to_bytes(2, "little"))
    out.extend(len(name).to_bytes(2, "little"))
    out.extend(len(payload).to_bytes(4, "little"))
    out.extend(name)
    out.extend(payload)


def put_content_entry(
    table: bytearray,
    index: int,
    content_index: int,
    role: int,
    offset: int,
    payload: bytes,
) -> None:
    entry = index * CONTENT_ENTRY_V0_LEN
    table[entry : entry + 2] = content_index.to_bytes(2, "little")
    table[entry + 2 : entry + 4] = role.to_bytes(2, "little")
    table[entry + 4 : entry + 6] = PACKAGE_CONTENT_FORMAT_BYTES.to_bytes(2, "little")
    table[entry + 6 : entry + 8] = (1).to_bytes(2, "little")
    table[entry + 8 : entry + 16] = offset.to_bytes(8, "little")
    table[entry + 16 : entry + 24] = len(payload).to_bytes(8, "little")
    table[entry + 24 : entry + 56] = hashlib.sha256(payload).digest()


def build_artifact(manifest: bytes, content_table: bytes, content: bytes) -> bytes:
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
    return artifact[:96] + digest + artifact[128:]


def require_file(path: Path, description: str) -> bytes:
    if not path.exists():
        raise AssertionError(f"missing {description}: {path}")
    return path.read_bytes()


def run(command: list[str]) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout, end="")
    if result.returncode != 0:
        raise AssertionError(f"{command} returned {result.returncode}")
    return result.stdout


def build_independent_seed_fixture() -> bytes:
    require_file(INDEPENDENT_SEED_SOURCE, "independent Phase 13 Pyth source")
    schema_descriptor = require_file(
        INDEPENDENT_SEED_SCHEMA, "independent Phase 13 schema descriptor"
    )
    TARGET.mkdir(parents=True, exist_ok=True)
    run(
        [
            "cargo",
            "run",
            "-p",
            "pythc",
            "--",
            "build",
            str(INDEPENDENT_SEED_SOURCE),
            "-o",
            str(INDEPENDENT_SEED_GRAPH),
        ]
    )
    run(["cargo", "run", "-p", "pyth-tig-tool", "--", "verify", str(INDEPENDENT_SEED_GRAPH)])
    graph_package = require_file(INDEPENDENT_SEED_GRAPH, "independent Phase 13 PythTIG graph")

    export_payload = (
        PACKAGE_EXPORT_KIND_TOOL.to_bytes(2, "little")
        + (1).to_bytes(2, "little")
        + (0).to_bytes(2, "little")
    )
    requirement_payload = (
        PACKAGE_LAUNCH_REQUIREMENT_ID.to_bytes(2, "little")
        + PACKAGE_LAUNCH_IMPORT_SLOT.to_bytes(2, "little")
        + PYTHTIG_RESOURCE_OBJECT_WORKSPACE.to_bytes(2, "little")
        + (0).to_bytes(2, "little")
        + PYTHTIG_RIGHTS_CREATE.to_bytes(8, "little")
    )
    manifest = bytearray(PACKAGE_MANIFEST_MAGIC)
    manifest.extend((3).to_bytes(4, "little"))
    push_manifest_record(
        manifest,
        MANIFEST_RECORD_SCHEMA_DECLARATION,
        b"seed.v0",
        (0).to_bytes(2, "little"),
    )
    push_manifest_record(
        manifest,
        MANIFEST_RECORD_PACKAGE_EXPORT,
        b"seed/launch",
        export_payload,
    )
    push_manifest_record(
        manifest,
        MANIFEST_RECORD_CAPABILITY_REQUIREMENT,
        b"seed/launch.requirement.object-create",
        requirement_payload,
    )

    content = schema_descriptor + graph_package
    content_table = bytearray(2 * CONTENT_ENTRY_V0_LEN)
    put_content_entry(
        content_table,
        0,
        0,
        PACKAGE_CONTENT_ROLE_SCHEMA_DESCRIPTOR,
        0,
        schema_descriptor,
    )
    put_content_entry(
        content_table,
        1,
        1,
        PACKAGE_CONTENT_ROLE_PYTHTIG_GRAPH,
        len(schema_descriptor),
        graph_package,
    )
    return build_artifact(bytes(manifest), bytes(content_table), content)


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


def manifest_records(manifest: bytes) -> list[tuple[int, bytes, bytes]]:
    if len(manifest) < 12 or manifest[:8] != PACKAGE_MANIFEST_MAGIC:
        raise AssertionError("bad independent manifest")
    count = int.from_bytes(manifest[8:12], "little")
    offset = 12
    records: list[tuple[int, bytes, bytes]] = []
    for _ in range(count):
        record_type = int.from_bytes(manifest[offset : offset + 2], "little")
        flags = int.from_bytes(manifest[offset + 2 : offset + 4], "little")
        name_len = int.from_bytes(manifest[offset + 4 : offset + 6], "little")
        payload_len = int.from_bytes(manifest[offset + 6 : offset + 10], "little")
        if flags != 0:
            raise AssertionError("independent manifest record has flags")
        offset += 10
        name = manifest[offset : offset + name_len]
        offset += name_len
        payload = manifest[offset : offset + payload_len]
        offset += payload_len
        records.append((record_type, name, payload))
    if offset != len(manifest):
        raise AssertionError("independent manifest has trailing bytes")
    return records


def self_test_independent_seed() -> None:
    artifact = build_independent_seed_fixture()
    if artifact[:8] != PACKAGE_ARTIFACT_MAGIC:
        raise AssertionError("bad independent fixture magic")
    manifest_offset = int.from_bytes(artifact[16:24], "little")
    manifest_len = int.from_bytes(artifact[24:32], "little")
    content_table_offset = int.from_bytes(artifact[32:40], "little")
    content_table_len = int.from_bytes(artifact[40:48], "little")
    manifest = artifact[manifest_offset : manifest_offset + manifest_len]
    if artifact[64:96] != hashlib.sha256(manifest).digest():
        raise AssertionError("bad independent manifest digest")
    if artifact[96:128] != hashlib.sha256(artifact_digest_domain(artifact)).digest():
        raise AssertionError("bad independent artifact digest")
    records = manifest_records(manifest)
    schemas = [
        record
        for record in records
        if record[0] == MANIFEST_RECORD_SCHEMA_DECLARATION
    ]
    exports = [
        record
        for record in records
        if record[0] == MANIFEST_RECORD_PACKAGE_EXPORT
    ]
    requirements = [
        record
        for record in records
        if record[0] == MANIFEST_RECORD_CAPABILITY_REQUIREMENT
    ]

    if len(schemas) != 1:
        raise AssertionError(f"expected one schema declaration, saw {len(schemas)}")
    if len(exports) != 1:
        raise AssertionError(f"expected one package export, saw {len(exports)}")
    if len(requirements) != 1:
        raise AssertionError(
            f"expected one capability requirement, saw {len(requirements)}"
        )
    if schemas[0][1] != b"seed.v0" or schemas[0][2] != (0).to_bytes(2, "little"):
        raise AssertionError("independent schema declaration does not target content 0")
    if exports[0][1] != b"seed/launch":
        raise AssertionError("independent export name is not seed/launch")
    if requirements[0][1] != b"seed/launch.requirement.object-create":
        raise AssertionError("independent requirement is not tied to seed/launch")
    export_kind = int.from_bytes(exports[0][2][0:2], "little")
    if export_kind != PACKAGE_EXPORT_KIND_TOOL:
        raise AssertionError("independent export is not launchable")
    requirement_payload = requirements[0][2]
    if len(requirement_payload) != 16:
        raise AssertionError("independent requirement payload size changed")
    requirement_id = int.from_bytes(requirement_payload[0:2], "little")
    import_slot = int.from_bytes(requirement_payload[2:4], "little")
    resource_kind = int.from_bytes(requirement_payload[4:6], "little")
    rights = int.from_bytes(requirement_payload[8:16], "little")
    if (
        requirement_id != PACKAGE_LAUNCH_REQUIREMENT_ID
        or import_slot != PACKAGE_LAUNCH_IMPORT_SLOT
        or resource_kind != PYTHTIG_RESOURCE_OBJECT_WORKSPACE
        or rights != PYTHTIG_RIGHTS_CREATE
    ):
        raise AssertionError("independent requirement is not object-create authority")
    forbidden = [
        field for field in FORBIDDEN_AUTHORITY_FIELDS if field in manifest.lower()
    ]
    if forbidden:
        raise AssertionError(f"forbidden authority field in manifest: {forbidden[0]!r}")
    if content_table_len != 2 * CONTENT_ENTRY_V0_LEN:
        raise AssertionError("independent fixture content entry count changed")
    content_table = artifact[content_table_offset : content_table_offset + content_table_len]
    if (
        int.from_bytes(content_table[0:2], "little") != 0
        or int.from_bytes(content_table[64:66], "little") != 1
    ):
        raise AssertionError("independent fixture content indexes changed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--fixture", choices=("format", "independent-seed"), default="format")
    args = parser.parse_args()

    if args.self_test:
        if args.fixture == "independent-seed":
            self_test_independent_seed()
        else:
            self_test()
        print("PHASE13_PACKAGE_FIXTURE_OK")
        return 0
    if args.output is None:
        raise SystemExit("--output is required unless --self-test is used")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    artifact = (
        build_independent_seed_fixture()
        if args.fixture == "independent-seed"
        else build_format_fixture()
    )
    args.output.write_bytes(artifact)
    print(f"PHASE13_PACKAGE_FIXTURE_READY {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
