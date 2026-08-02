# Lab Lang: initial grammar

The first Lab Lang surface is intentionally small. It specifies a physical artifact and its acceptance criteria without choosing a robot, plate layout, pipette, assembly method, or propagation host.

```lab
plasmid p_sensor {
  sequence "ATGCGTACGTTAGCTA";
  topology circular;
  copies 1;

  acceptance {
    exact_sequence;
    minimum_concentration 100 ng_per_ul;
    minimum_volume 20 ul;
  }
}
```

Supported statements are:

- `sequence "...";` with an unambiguous DNA sequence (`A`, `C`, `G`, `T`);
- `topology circular;`;
- `copies <positive integer>;`;
- `exact_sequence;` inside `acceptance`;
- `minimum_concentration <integer> ng_per_ul;` inside `acceptance`;
- `minimum_volume <integer> ul;` inside `acceptance`.

Line comments begin with `//`. Exact sequence verification is currently required because the implemented plasmid pipeline cannot call an artifact validated without sequence evidence. The frontend represents any positive copy count, while the first plasmid compiler lowering deliberately fails closed for counts other than one until replicate material flow is modeled explicitly.

The [plasmid acceptance example](../../../examples/plasmid-acceptance/README.md) follows a Lab Lang source through the current compiler pipeline.
