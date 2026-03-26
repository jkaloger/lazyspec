---
title: "Document Numbering and Reservations"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [engine, numbering, sqids, reservations]
related: []
---

## Summary

Every document created by lazyspec needs a unique identifier embedded in its filename. The engine supports three numbering strategies -- incremental, sqids, and reserved -- each configured per document type in `.lazyspec.toml`. The `resolve_filename` function is the single entry point that dispatches to the appropriate strategy and produces the final filename by substituting pattern placeholders.

## Numbering Strategies

The `NumberingStrategy` enum has three variants: `Incremental` (default), `Sqids`, and `Reserved`. Each type definition in `[[types]]` declares its strategy via the `numbering` field. When no value is set, `Incremental` is used.

@ref src/engine/config.rs#NumberingStrategy

### Incremental

`next_number` scans the target directory for files matching the type prefix, extracts the numeric segment from each filename, and returns one greater than the highest number found. If no matching files exist, it returns 1. The number is zero-padded to three digits when the pattern contains `{n:03}`.

@ref src/engine/template.rs#next_number

### Sqids

Sqids-based numbering produces short, non-sequential alphanumeric identifiers. The `next_sqids_id` function seeds the sqids encoder with the current Unix timestamp, encodes it, lowercases the result, and checks whether a file with that prefix already exists in the directory. If a collision is detected, the input is incremented and the loop retries until a unique ID is found.

@ref src/engine/template.rs#next_sqids_id

Configuration lives in a global `[numbering.sqids]` section. Two fields are required when any type uses sqids: `salt` (a non-empty string) and `min_length` (an integer between 1 and 10, defaulting to 3).

@ref src/engine/config.rs#SqidsConfig

### Alphabet Shuffling

The `shuffle_alphabet` function deterministically reorders the default sqids alphabet using the configured salt. It iterates the alphabet in reverse, computing swap indices from salt byte values. An empty salt returns the alphabet unchanged. This ensures that different projects produce different ID sequences from the same numeric input.

@ref src/engine/template.rs#shuffle_alphabet

## Filename Resolution

`resolve_filename` is the central function that assembles a document filename from a naming pattern. It slugifies the title (lowercase, non-alphanumeric characters replaced with hyphens, consecutive hyphens collapsed), formats the current date, uppercases the document type, and substitutes the `{type}`, `{title}`, `{date}`, `{n:03}`, and `{n}` placeholders.

@ref src/engine/template.rs#resolve_filename

When the pattern contains no number placeholder (`{n:03}` or `{n}`), the function returns immediately without consulting any numbering strategy. If a `pre_computed_id` is provided (as happens with reserved numbering), it is substituted directly. Otherwise, the function dispatches to `next_sqids_id` or `next_number` based on the strategy.

@ref src/engine/template.rs#slugify

## Reserved Numbering

The `Reserved` strategy coordinates number allocation across distributed contributors using git custom refs under the `refs/reservations/{PREFIX}/{NUM}` namespace. Unlike incremental and sqids, reservation requires network access to the git remote.

@ref src/engine/reservation.rs#reserve_next

The `reserve_next` function follows this protocol:

1. Query the remote with `git ls-remote --refs` to discover existing reservations for the prefix.
2. Compute the candidate as one greater than the maximum of the remote reservations and the local filesystem scan.
3. Create a local ref by writing an empty blob with `git hash-object` and pointing a ref at it with `git update-ref`.
4. Attempt an atomic `git push` of the ref. If the push succeeds, the number is reserved.
5. If the push is rejected (another contributor claimed the same number), clean up the local ref, increment the candidate, and retry.
6. If all retries are exhausted, fail with an error listing the range of attempted numbers.

@ref src/engine/reservation.rs#ReservationProgress

The `ReservationProgress` enum provides structured progress callbacks at each stage: querying the remote, each push attempt, push rejections, and the final reserved number.

### Configuration

Reserved numbering is configured in `[numbering.reserved]` with three fields: `remote` (defaults to `"origin"`), `format` (either `incremental` or `sqids`), and `max_retries` (defaults to 5). The `format` field controls how the raw reserved integer is rendered in the filename -- as a zero-padded number or as a sqids-encoded string.

@ref src/engine/config.rs#ReservedConfig

When `format = "sqids"`, validation requires a `[numbering.sqids]` section with a non-empty salt. When `format = "incremental"`, no sqids configuration is needed.

## Reservation Management

Two operations support lifecycle management of reservation refs.

`list_reservations` queries the remote for all refs under `refs/reservations/*`, parses each into a `Reservation` struct (prefix, number, ref path), and returns them. Unreachable remotes produce an error that inspects stderr for connection-related keywords.

@ref src/engine/reservation.rs#list_reservations
@ref src/engine/reservation.rs#Reservation

`delete_remote_ref` removes a single reservation ref from the remote via `git push --delete`. This is the primitive used by the `reservations prune` subcommand, which deletes refs whose corresponding documents already exist locally and flags orphaned refs (reservations with no matching local file) without deleting them.

@ref src/engine/reservation.rs#delete_remote_ref
