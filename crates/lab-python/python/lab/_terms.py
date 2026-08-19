"""The ontology terms the SBOL bridge reads, and how they are compared.

SBOL states what a component is as identifiers.org IRIs over SBO, SO, and
friends. Tools write those IRIs with either scheme and occasionally as a bare
CURIE, so comparison happens on the `PREFIX:NUMBER` tail rather than the full
IRI. These are the same terms `std.bio.ontology` grounds Lab's kinds in.
"""

from __future__ import annotations

import re
from collections.abc import Iterable

SIMPLE_CHEMICAL = "SBO:0000247"
NUCLEIC_ACID = "SBO:0000251"
MACROMOLECULE = "SBO:0000252"

PROMOTER = "SO:0000167"
RIBOSOME_ENTRY_SITE = "SO:0000139"
CDS = "SO:0000316"
TERMINATOR = "SO:0000141"
ENGINEERED_REGION = "SO:0000804"
CIRCULAR = "SO:0000988"
LINEAR = "SO:0000987"

#: The SBOL 3 `meets` constraint: the subject's end abuts the object's start,
#: which is how an SBOL document states physical order.
MEETS = "http://sbols.org/v3#meets"

_CURIE = re.compile(r"(?:^|/)([A-Za-z]+:\d+)$")


def term(iri: object) -> str:
    """The `PREFIX:NUMBER` tail of an ontology IRI, or the text as given."""

    text = str(iri)
    match = _CURIE.search(text)
    return match.group(1) if match else text


def terms(iris: Iterable[object]) -> set[str]:
    return {term(iri) for iri in iris}
