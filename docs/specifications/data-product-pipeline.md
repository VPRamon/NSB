# Dataset pipeline architecture

`nsb-data-tools` has one executable and one public abstraction: a versioned
dataset lifecycle. CLI adapters parse configuration and call the Rust dataset
engine; scientific transforms, validation and persistence never spawn another
NSB or Cargo process.

## Contracts

`DatasetName`, `BuildPlan`, `Artifact`, `ValidationReport` and `RunManifest`
are typed, versioned Rust contracts. Persisted JSON rejects unknown fields.
Every artifact records its path, byte count and SHA-256. Every run pins its
resolved workspace, configuration checksum, Git commit, executor and exact
partition selection.

Configuration paths are explicit. Relative values resolve against the TOML
file, making behavior independent of the caller's current directory. There are
no personal, removable-media or environment-derived storage defaults.

## Lifecycle

The supported order is:

1. `update` verifies source checksums and atomically stages inputs;
2. `build` performs deterministic Rust transformations;
3. `validate` recomputes integrity and scientific/format gates;
4. `publish` rechecks validated bytes, copies them atomically and updates the
   runtime manifest without committing.

Missing, skipped or failed validation is never a pass. A changed artifact
invalidates publication. Run manifests record failures and can be inspected or
resumed without changing their dataset, operation or partitions.

## Local and Slurm execution

The local executor runs the dataset engine directly. Starlight may instead
submit a Slurm array whose tasks invoke the hidden worker in the same Rust
binary. Stable partition identifiers, isolated worker directories and
checksum-pinned state make completion order irrelevant. No maintained shell or
Python orchestration is permitted.

The initial storage contract is a caller-selected POSIX filesystem. Object
storage and other schedulers are outside the current interface.
