"""Check every module in the Golden Gate Python example."""

import lab

from .designs import inventory, plasmids, strains
from .programs import reporter_panel
from .workflows import assemble, build_strains

MODULES = (
    inventory.module,
    plasmids.module,
    strains.module,
    assemble.module,
    build_strains.module,
    reporter_panel.module,
)


def main() -> None:
    program = lab.check(*MODULES)
    print(f"Checked Golden Gate Python example ({len(program.checked)} modules)")


if __name__ == "__main__":
    main()
