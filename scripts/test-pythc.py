import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tools" / "pythc" / "tests" / "fixtures"
TARGET = ROOT / "target" / "pythc-acceptance"


def run(args, expect=0):
    completed = subprocess.run(
        args,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode != expect:
        sys.stderr.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise SystemExit(f"{args} exited {completed.returncode}, expected {expect}")
    return completed


def cargo_run(package, *args, expect=0):
    return run(["cargo", "run", "-q", "-p", package, "--", *args], expect=expect)


def main():
    TARGET.mkdir(parents=True, exist_ok=True)
    hello = FIXTURES / "hello.pyth"
    out1 = TARGET / "hello-1.pytig"
    out2 = TARGET / "hello-2.pytig"

    cargo_run("pythc", "build", str(hello), "-o", str(out1))
    cargo_run("pythc", "build", str(hello), "-o", str(out2))
    if out1.read_bytes() != out2.read_bytes():
        raise SystemExit("pythc build output is not deterministic")

    verify = cargo_run("pyth-tig-tool", "verify", str(out1))
    if "PYTH_TIG_VERIFY_OK" not in verify.stdout:
        raise SystemExit("pyth-tig-tool did not verify pythc output")

    inspect = cargo_run("pythc", "inspect", str(out1)).stdout
    required = [
        "program: hello",
        "principal: 0x5059544847520001",
        "imports:",
        "blocks:",
        "nodes:",
        "checksum:",
    ]
    for text in required:
        if text not in inspect:
            raise SystemExit(f"missing inspect field: {text}")

    negatives = {
        "negative-revise-rights.pyth": "T0012",
        "negative-unbudgeted-while.pyth": "P0007",
        "negative-unknown-name.pyth": "T0002",
    }
    for fixture, code in negatives.items():
        result = cargo_run("pythc", "build", str(FIXTURES / fixture), "-o", str(TARGET / "bad.pytig"), expect=2)
        output = result.stdout + result.stderr
        if f"error[{code}]" not in output:
            raise SystemExit(f"{fixture} did not report {code}")

    print("PYTHC_TEST_OK")


if __name__ == "__main__":
    main()
