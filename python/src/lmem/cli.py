from __future__ import annotations
import sys
import subprocess
from .binary import get_binary_path


def main() -> None:
    """CLI entrypoint for lmem Python package."""
    try:
        binary = get_binary_path()
    except FileNotFoundError as e:
        sys.stderr.write(f"Error: {e}\n")
        sys.exit(1)

    result = subprocess.call([binary] + sys.argv[1:])
    sys.exit(result)


if __name__ == "__main__":
    main()
