# Data-product pipeline architecture

This document defines the production boundary for retained NSB data-product tools. It is normative for new Gaia acquisition, Starlight generation, validation, packing, and cleanup workflows.

## Dependency direction

Executables are adapters. A supported binary may parse arguments, initialize logging, construct typed configuration, call a library service, serialize a stable machine result, and translate a typed decision into an exit code. It must not own a scientific algorithm, a persisted state machine, or a second implementation of a schema/checksum contract.

The reusable library boundary is split as follows:

- `pipeline::contracts`: versioned processing modes, explicit row coverage, completion evidence, and gate outcomes;
- `pipeline::admission`: fail-closed production admission and deterministic exit status;
- `pipeline::checkpoint`: compact partition-oriented resume state with bounded diagnostics;
- `pipeline::state`: evidence-driven cache transitions and recovery action for every persisted state;
- `pipeline::store`: transactional persistence for strict partition-state records;
- `pipeline::reconciliation`: canonical ordering, duplicate rejection, and checked aggregate accounting;
- `artifact_io`: transactional JSON/byte persistence;
- `checksum_io`: algorithm-qualified streaming checksum authority;
- scientific Gaia/Starlight modules: transformation and validation algorithms only.

Dependencies flow from executables to services, from services to pipeline/scientific modules, and from those modules to persistence/checksum primitives. Scientific modules must not spawn executables. Every command retained in the tool registry must enter through a documented library service and a thin executable adapter.

## Typed coverage and run intent

Full-file processing is represented by `RowSelection::FullPartition`. A bounded run is `RowSelection::FirstRows(n)` and rejects `n = 0`. No command or report may use zero as a context-dependent sentinel.

`ProcessingMode` distinguishes pilot, candidate, and production work. Pilot/candidate outputs may be scientifically useful, but cannot authorize deletion or production promotion.

`PartitionCompletion` distinguishes an observed partial prefix from a durable end-of-partition result. A bounded selection cannot create complete-partition evidence.

## Gate and exit-code contract

A gate is one of:

- `Passed`: executed and successful;
- `Failed(reason)`: executed and unsuccessful;
- `NotRun(reason)`: not executed and therefore not a pass;
- `NotApplicable(reason)`: explicitly outside the operation and not an executed pass.

Every gate required for production must be `Passed`. Explicit blockers always reject admission. `ProductionAdmission::evaluate` returns exit code `0` only for `Ready`; every blocked decision maps to exit code `2`.

Human-readable diagnostics belong on stderr or in structured reports. Stable machine output must not be contaminated by lifecycle logging.

## Transaction and cleanup order

The durable order for a production partition is:

1. acquire the input into a temporary file;
2. flush and atomically promote the download;
3. recompute and compare the official checksum;
4. process with a compact checkpoint;
5. flush and atomically promote the output;
6. verify the output checksum and structure;
7. persist the partition reconciliation manifest;
8. persist aggregate reconciliation/merge state;
9. transition the input to `Releasable` only after all release evidence is present;
10. optionally delete the source input and persist `Deleted`.

A crash between any two boundaries resumes through `PartitionState::resume_action`. The state machine covers `Planned`, `Downloading`, `Downloaded`, `ChecksumVerified`, `Processing`, `Processed`, `OutputVerified`, `Reconciled`, `Releasable`, `Deleted`, and `Failed` explicitly. `write_partition_state` validates before an atomic write, and `read_partition_state` rejects corrupted or incompatible records before recovery proceeds.

`Releasable` requires all of the following:

- production mode;
- full-partition row selection;
- complete-partition evidence;
- verified official input checksum;
- verified output checksum;
- committed reconciliation checksum.

Partial, pilot, or candidate processing cannot satisfy this contract.

## Deterministic reconciliation

Each `PartitionManifest` proves full production coverage, exact row classification, the official input checksum, a SHA-256 output checksum, and a SHA-256 HEALPix checksum. `ReconciliationManifest` sorts partitions by immutable identifier, rejects duplicates, uses checked arithmetic for every aggregate, and validates persisted totals against the partition set.

Canonical JSON is therefore independent of partition completion order or worker concurrency. Duplicate partitions, partial runs, inconsistent counts, unknown fields, unsupported schemas, and modified aggregate totals fail closed.

## Checkpoint scalability

Production checkpoints are partition-oriented. They retain row offsets, aggregate counters, rolling/checkpoint checksums, HEALPix checkpoint references, and at most 32 representative diagnostic samples. They do not persist every source ID or a global per-source flux map.

The checkpoint size is therefore bounded by fixed metadata and HEALPix/partition state rather than the Gaia source population. Exact reconciliation that needs source-level ordering must use streamed or external sorted structures owned by the reconciliation stage, not a global in-memory checkpoint.

## Persisted schema policy

Pipeline records use `schema_version = 1`, typed Rust structs, and `serde(deny_unknown_fields)`. Readers reject unsupported versions, missing required fields, unknown fields, contradictory completion evidence, invalid zero limits, empty diagnostics, and states lacking their required evidence.

A future incompatible format increments the schema version and adds an explicit migration. Silent defaults for renamed or missing production fields are forbidden.

## Required validation

Changes to production orchestration must include tests proving:

- zero cannot represent both bounded and full processing;
- skipped required gates block admission;
- every blocker produces a non-zero exit code;
- checksum mismatch leaves state unmodified;
- pilot or partial processing cannot become releasable;
- complete production evidence can become releasable;
- every persisted state has a deterministic resume action;
- every durable state round-trips transactionally;
- corrupted and unknown state fields fail closed;
- reconciliation is invariant to processing order;
- duplicate, partial, and inconsistent partition manifests are rejected;
- checkpoint diagnostics and serialized size remain bounded;
- unknown fields and unsupported schema versions fail closed.

The architecture, recovery, reconciliation, and pipeline contract tests are mandatory release gates for every retained orchestration change.
