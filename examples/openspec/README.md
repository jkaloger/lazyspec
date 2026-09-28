# openspec pack

A lazyspec workflow pack in the shape of [OpenSpec](https://github.com/Fission-AI/OpenSpec): a `change` describes a proposed change as one folder, and a `delta` records the detailed spec diff for one capability it affects.

## Layout

| Path | Role |
| --- | --- |
| `.lazyspec.toml` | The pack's config: two types (`change`, `delta`), the `implements` relationship, and one edge nudging a delta to link back to its change. |
| `.lazyspec/templates/change/` | A directory template. `index.md` is the change document itself; `proposal.md`, `design.md` and `tasks.md` are its parts. |
| `.lazyspec/templates/delta.md` | The flat template for a `delta`. |
| `README.md` | This file. |

## Adoption

`lazyspec init --template <path-to-this-directory>` copies `.lazyspec.toml` and `.lazyspec/templates/` into the current project. `--force` overwrites an existing `.lazyspec.toml`; without it, `init` refuses when one is already present. Document storage stays local: adopting this pack sets no `extends` and moves nothing.

## Documents

A `change` is created with `lazyspec create change <title>`, which scaffolds `changes/CHANGE-NNN-<slug>/index.md`, `proposal.md`, `design.md` and `tasks.md` in one call. `index.md` carries a one-paragraph summary; `proposal.md` states why the change is needed, what it changes and its impact; `design.md` records the decision behind it; `tasks.md` is the build breakdown.

A `delta` is created per affected capability with `lazyspec create delta <capability> --parent <change-id>`, which lands it as a sibling file inside the change's own folder. A delta states which of ADDED, MODIFIED or REMOVED it is, and the capability's behaviour before and after.

The `deltas-need-changes` edge reports a warning when a delta carries no `implements` link back to its change; it does not block `create` or `validate`.
