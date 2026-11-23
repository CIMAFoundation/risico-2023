# API Reference

The API reference is generated from the source with **rustdoc** and published with this
site:

<div class="hero__actions" markdown>
[Open the API reference :material-arrow-right:](api/risico/index.html){ .md-button .md-button--primary }
</div>

It documents the `risico` library crate, the models without the runner. Start from the
crate page: it explains the concepts and units, with a tested example. The main modules:

| Module | Contents |
| --- | --- |
| [`risico::models::input`](api/risico/models/input/index.html) | `Input`, `InputElement`: the weather of one timestamp; `InputVariableName`: the variable names of input files |
| [`risico::models::output`](api/risico/models/output/index.html) | `Output`, `OutputElement`, `OutputVariableName`: the output variables of every model |
| [`risico::modules::risico`](api/risico/modules/risico/index.html) | RISICO: properties, vegetation, warm state, state, the `legacy` / `v2023` / `v2025` formulations |
| [`risico::modules::fwi`](api/risico/modules/fwi/index.html) | Canadian FWI System: properties, warm state, state, the `legacy` / `cffdrs` / `sliding_window` formulations |
| [`risico::modules`](api/risico/modules/index.html) | All models: KBDI, Mark 5, Angström, Fosberg, Nesterov, Sharples, Orieux, HDW |
| [`risico::config`](api/risico/config/index.html) | The `configuration.yml` schema (feature `config`) |
| [`risico::constants`](api/risico/constants/index.html) | Shared constants, among them `NODATAVAL` |

To build it locally:

```bash
cargo doc --no-deps --lib --open
```

For a guided tour with complete programs see the [Programmatic Workflow](programmatic.md).
