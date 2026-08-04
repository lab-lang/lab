# 0002: Reactive durable workflows

Status: accepted, frontend implemented; runtime pending

Lab workflows are deterministic durable state machines whose effects occur in the physical world. The source language should read sequentially when the scientific process is sequential; durability does not require an `await` marker on every line.

`when` introduces event-driven behavior. Timers are events, expressed as `when every <duration>` and `when after <duration>`. Effects use `<-`, and their results are values produced from recorded completion events.

The runtime must eventually provide command identities, idempotent dispatch, event journaling, durable timers, replay, explicit physical cancellation, capability providers, and preservation of material linearity across branches.

The current frontend parses and type-checks reactive handlers, lowers their typed structure into portable module IR, and verifies captured material ownership. It does not yet dispatch or durably replay a workflow.
