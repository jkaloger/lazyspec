---
title: "{title}"
type: {type}
status: draft
author: "{author}"
date: {date}
tags: []
---

<!--
Link this delta back to its change so it isn't reported as orphaned, and to
the capability's main spec when it modifies an existing one:

    related:
    - implements: CHANGE-NNN
    - related-to: SPEC-NNN
-->

# Spec Delta

## Purpose
<!-- New capabilities only: one or two sentences (50+ characters) on what this capability is for. Delete this section for an existing capability. -->

## ADDED Requirements

### Requirement: <!-- requirement name -->
<!-- requirement text -->

#### Scenario: <!-- scenario name -->
- **WHEN** <!-- condition -->
- **THEN** <!-- expected outcome -->

<!--
Other delta sections, each a `##` heading, used only when they apply:

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

Format rules: every requirement is `### Requirement: <name>` and uses
SHALL/MUST; every requirement has at least one `#### Scenario:` (exactly four
hashes) with WHEN/THEN bullets.
-->
