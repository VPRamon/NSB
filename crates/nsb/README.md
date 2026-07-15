# nsb

Runtime Rust library for ground-based Night Sky Background modelling.

This crate owns the typed scientific API: component models, point evaluation,
and NSB threshold-window search. It intentionally does not contain command-line
parsing, named-site aliases, or operational output formatting; those live in the
sibling `nsb-cli` crate.
