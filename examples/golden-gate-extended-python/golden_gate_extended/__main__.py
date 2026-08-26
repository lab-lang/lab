"""Check every module in the extended Golden Gate Python example."""

import lab

from .designs import circuits, inventory, parts, plasmids, strains
from .programs import panel
from .workflows import assemble, build_strains, observe

MODULES = (
    parts.module,
    inventory.module,
    plasmids.module,
    strains.module,
    circuits.module,
    assemble.module,
    build_strains.module,
    observe.module,
    panel.module,
)


def main() -> None:
    program = lab.check(*MODULES)
    print(f"Checked extended Golden Gate Python example ({len(program.checked)} modules)")


if __name__ == "__main__":
    main()
