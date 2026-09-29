# Proposal

## Why

<!-- Explain the motivation for this change. What problem does this solve? Why now? -->

## What Changes

<!-- Describe what will change. Be specific about new capabilities, modifications, or removals. Mark breaking changes with **BREAKING**. -->

## Capabilities

### New Capabilities
<!-- Capabilities being introduced. Use kebab-case for path segments you introduce
     (e.g., user-auth or identity/user-auth) that follow the project's existing
     spec organization. Each gets a delta spec: a frontmatter-less
     `<capability-path>.md` in this change's folder, starting `# Spec Delta`. -->
- `<capability-path>`: <brief description of what this capability covers>

### Modified Capabilities
<!-- Existing capabilities whose REQUIREMENTS are changing (not just implementation).
     Only list here if spec-level behavior changes. Each needs a delta spec file.
     Use the exact existing capability's title under openspec/specs/. Leave empty
     if no requirement changes. Do not invent a requirement just to fill this in. -->
- `<existing-capability-path>`: <what requirement is changing>

## Impact

<!-- Affected code, APIs, dependencies, systems -->

<!--
Delta spec format, one file per capability listed above:

    # Spec Delta

    ## Purpose
    New capabilities only: one or two sentences on what the capability is for.
    Delete this section for an existing capability.

    ## ADDED Requirements

    ### Requirement: <name>
    The system SHALL ...

    #### Scenario: <name>
    - **WHEN** <condition>
    - **THEN** <expected outcome>

    ## MODIFIED Requirements
    Copy the ENTIRE existing requirement block (`### Requirement:` through
    every scenario) from the capability's spec, then edit it to the new
    behaviour. The header text must match the existing one exactly.

    ## REMOVED Requirements
    ### Requirement: <name>
    **Reason**: <why it is going>
    **Migration**: <what replaces it>

    ## RENAMED Requirements
    - FROM: `### Requirement: <old name>`
    - TO: `### Requirement: <new name>`

Use only the sections that apply. Every requirement uses SHALL/MUST and has at
least one `#### Scenario:` (exactly four hashes) with WHEN/THEN bullets.
-->
