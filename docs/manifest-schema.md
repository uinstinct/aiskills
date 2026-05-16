# manifest.yml schema

Every entry in the instinctagents registry — both **skills** (`assets/skills/<name>/manifest.yml`)
and **agents.md integrations** (`assets/agents.md/<name>/manifest.yml`) — ships with a
`manifest.yml`. This file is the single source of truth that the build script
(US-005) reads when baking the offline catalog into the CLI binary.

A `manifest.yml` is a small YAML document at the root of each registry entry's
folder. The build process fails closed: a missing or malformed manifest stops
the build with an error identifying the offending file.

## Shared fields (skills and agents.md integrations)

| Field                   | Type             | Required | Default | Description                                                                                                                            |
| ----------------------- | ---------------- | -------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `name`                  | string           | yes      | —       | Identifier for this entry. **Must match the folder name exactly** (e.g. `assets/skills/foo/manifest.yml` requires `name: foo`).        |
| `description`           | string           | yes      | —       | One-line human-readable description shown in the TUI catalog row.                                                                       |
| `version`               | string (semver)  | yes      | —       | Semantic version (e.g. `0.1.0`). Used by the release packager (US-029) to tag tarballs and by the installer to record installed state. |
| `harness_compatibility` | array of strings | no       | `[]`    | Subset of `claude-code`, `codex`, `opencode`. Empty list or omitted means **compatible with all harnesses**.                            |

### `harness_compatibility` values

Only these three string values are accepted:

- `claude-code`
- `codex`
- `opencode`

Any other value is a validation error. The CLI's Add tab (US-012) dims and
disables rows whose `harness_compatibility` does not include the detected
harness, unless the user passes `--force` (US-016).

## Skill-only field

| Field        | Type   | Required | Default     | Description                                                                                                |
| ------------ | ------ | -------- | ----------- | ---------------------------------------------------------------------------------------------------------- |
| `entrypoint` | string | no       | `SKILL.md`  | Path (relative to the skill folder) to the primary instruction file the agent should read for this skill. |

## agents.md-only field

| Field          | Type   | Required | Default       | Description                                                                                                                 |
| -------------- | ------ | -------- | ------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `snippet_file` | string | no       | `snippet.md`  | Path (relative to the integration folder) to the snippet that the installer injects into the harness instruction file (US-011). |

## Example — skill manifest

`assets/skills/my-skill/manifest.yml`:

```yaml
name: my-skill
description: One-line description of what this skill does.
version: 0.1.0
harness_compatibility: []
entrypoint: SKILL.md
```

Equivalent minimal form (relying on defaults):

```yaml
name: my-skill
description: One-line description of what this skill does.
version: 0.1.0
```

## Example — agents.md integration manifest

`assets/agents.md/my-integration/manifest.yml`:

```yaml
name: my-integration
description: One-line description of what this integration adds to AGENTS.md.
version: 0.1.0
harness_compatibility: []
snippet_file: snippet.md
```

## Example — harness-restricted manifest

A skill that only makes sense under Claude Code:

```yaml
name: claude-only-skill
description: Demonstrates a skill that targets a single harness.
version: 1.2.0
harness_compatibility:
  - claude-code
```

## Validation rules (build-time)

The build script enforces:

1. The file is valid YAML.
2. All required fields are present and non-empty.
3. `name` equals the parent folder's basename.
4. `version` parses as semver (`MAJOR.MINOR.PATCH`, optional pre-release).
5. Every entry in `harness_compatibility` is one of the three allowed values.
6. `entrypoint` (skills) or `snippet_file` (agents.md) — when present — points to a file that exists in the folder.

A failure on any of these stops the cargo build with an error message that
includes the offending manifest's path.
