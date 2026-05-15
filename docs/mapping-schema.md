# mapping.yml schema

The top-level [`mapping.yml`](../mapping.yml) at the root of the registry repo
is the **catalog index**: a single YAML document that enumerates every skill
and every agents.md integration currently published in the registry.

The internal ingest scripts (US-020 through US-024) produce and consume this
file deterministically — they read it to know what already exists, and they
write it back to record additions and removals. The Rust build script (US-005)
walks the per-entry `manifest.yml` files referenced by `install_path` to bake
the offline catalog into the CLI binary, so the shape of this file must stay
machine-parseable.

See [`docs/manifest-schema.md`](./manifest-schema.md) for the per-entry
`manifest.yml` schema that complements this index.

## Top-level structure

`mapping.yml` has exactly two top-level keys, both required (use an empty list
`[]` rather than omitting either):

| Key                   | Type            | Required | Description                                                              |
| --------------------- | --------------- | -------- | ------------------------------------------------------------------------ |
| `installed_skills`    | array of entry  | yes      | Every skill published in `skills/<name>/`.                              |
| `installed_agents_md` | array of entry  | yes      | Every agents.md integration published in `agents.md/<name>/`.            |

The key prefix `installed_` reflects the registry's perspective: an entry is
"installed in the catalog." It does **not** describe what is installed inside
any consumer project — that is tracked separately by the CLI in each
project's `.instinctagents` state file (US-009).

## Entry fields

Every entry under either list has the same four fields. All are required.

| Field          | Type            | Required | Description                                                                                                                                                            |
| -------------- | --------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`         | string          | yes      | Identifier for the entry. Must match the entry's folder basename and the `name` field of its `manifest.yml`.                                                            |
| `version`      | string (semver) | yes      | Semantic version. Must match the `version` field of the entry's `manifest.yml`; the ingest scripts copy it from the manifest.                                          |
| `source_url`   | string (URL)    | yes      | Upstream URL the entry was ingested from (GitHub repo, folder, or raw file URL).                                                                                       |
| `install_path` | string (path)   | yes      | Path relative to the registry repo root, **with trailing slash**, pointing at the entry folder. For skills: `skills/<name>/`. For agents.md: `agents.md/<name>/`.       |

### Constraints

1. `name` is unique **within a list**, but a skill and an agents.md
   integration may share a name (the CLI disambiguates with `--type`; see
   US-016).
2. `install_path` always begins with `skills/` for entries under
   `installed_skills`, and `agents.md/` for entries under
   `installed_agents_md`.
3. The folder pointed to by `install_path` must exist and must contain a
   valid `manifest.yml` whose `name` and `version` match this entry. The
   build script (US-005) is the enforcement point.
4. `source_url` is informational; the registry does not re-fetch from it at
   build time.

## Example

A minimal valid `mapping.yml` with one skill and one agents.md integration:

```yaml
installed_skills:
  - name: my-skill
    version: 0.1.0
    source_url: https://github.com/example/my-skill
    install_path: skills/my-skill/

installed_agents_md:
  - name: my-integration
    version: 0.1.0
    source_url: https://github.com/example/my-integration
    install_path: agents.md/my-integration/
```

An empty-but-valid `mapping.yml` (useful as a starting point for a fresh
registry fork):

```yaml
installed_skills: []
installed_agents_md: []
```

## How the schema is used

- **Internal scripts** (`src/internal/skill_add.py`, `skill_remove.py`,
  `agents_md_add.py`, `agents_md_remove.py`, `*_list.py`): load
  `mapping.yml`, mutate the relevant list, write it back. The list scripts
  treat it as read-only.
- **Build script** (`src/external/build.rs`): walks the `install_path` of
  every entry, opens each `manifest.yml`, and aggregates them into the
  baked-in catalog the CLI ships with.
- **Release packager** (`scripts/package-release-assets.sh`, US-029): uses
  the `name` + `version` pair to name the per-entry tarballs that get
  attached to GitHub Releases.

## Validation

There is no separate validator script for `mapping.yml`; the build script
catches schema violations indirectly. A `mapping.yml` is considered well-formed
when all of the following hold:

1. The file is valid YAML.
2. Both `installed_skills` and `installed_agents_md` are present as lists
   (possibly empty).
3. Every entry has all four required fields, non-empty, with the correct
   types.
4. `install_path` resolves to an existing folder containing a valid
   `manifest.yml` (per `docs/manifest-schema.md`).
5. The `name` and `version` of each entry match the corresponding fields in
   the referenced manifest.
