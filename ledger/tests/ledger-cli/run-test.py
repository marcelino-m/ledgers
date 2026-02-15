import argparse
import difflib
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import TextIO


def read_input(file: TextIO):
    input = []
    pos = 0
    line = file.readline()
    while line:
        if line.startswith("test "):
            file.seek(pos)
            break
        input.append(line)
        pos = file.tell()
        line = file.readline()
    return "".join(input)


@dataclass
class Test:
    command: str
    output: str
    exitcode: int


def read_test(file: TextIO) -> Test:
    in_output = False

    line = file.readline()

    command = None
    expected = []
    exitcode = 0

    while line:
        if line.startswith("test "):
            command = line[5:]
            match = re.match(r"(.*) -> ([0-9]+)", command)
            if match:
                command = match.group(1)
                exitcode = int(match.group(2))
            else:
                command = command
            command = command.rstrip()
            in_output = True

        elif in_output:
            if line.startswith("end test"):
                in_output = False
                break
            else:
                expected.append(line)
        line = file.readline()

    test = Test(command=command, output="".join(expected), exitcode=exitcode)

    return test.command and test


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run test")

    parser.add_argument(
        "--test", type=Path, required=True, help="Path to the test file"
    )

    parser.add_argument(
        "--ledger", type=Path, required=True, help="Path to the ledger executable"
    )

    parser.add_argument(
        "--print-output",
        required=False,
        action="store_true",
        help="if true, only print the output of the command",
    )

    args = parser.parse_args()
    if not args.test.exists():
        parser.error(f"Test file does not exist: {args.test}")

    if not args.ledger.exists():
        parser.error(f"Ledger cmd does not exist: {args.ledger}")

    # Open and read test file
    test_file = open(args.test, "r")
    input = read_input(test_file)

    test = read_test(test_file)
    # print("expected:", test.output)
    ntest = 0
    while test:
        ntest += 1
        result = subprocess.run(
            [args.ledger] + test.command.split(),
            input=input,
            text=True,
            capture_output=True,
        )
        if result.returncode != test.exitcode:
            eprint(f"Test {args.test} N={ntest} failed: {test.command}")
            eprint(f"Expected exit code: {test.exitcode}, got: {result.returncode}")
            eprint(f"Output command:\n{result.stderr}")
            sys.exit(1)

        test_output = test.output.splitlines(keepends=True)
        cmd_output = [
            s[1:].rstrip() + "\n" for s in result.stdout.splitlines(keepends=True)
        ]
        if args.print_output:
            print("".join(cmd_output))
        else:
            if cmd_output != test_output:
                diff = difflib.unified_diff(
                    test_output,
                    cmd_output,
                    fromfile="expected",
                    tofile="actual",
                )
                eprint(f"Test {args.test} N={ntest} failed: {test.command}")
                sys.stdout.writelines(diff)
                sys.exit(1)

        test = read_test(test_file)
