//! Shared implementation for NSB maintainer data-product tools.

// `gaia::acquisition::storage_preflight` needs `libc::statvfs` for USB/NVMe capacity gates.
#![deny(unsafe_code)]

extern crate self as nsb_data_tools;

pub mod cli;
pub mod gaia;
pub mod platform;
pub mod starlight;

use clap::Parser;
use std::cell::RefCell;
use std::ffi::OsString;

thread_local! {
    static COMMAND_ARGUMENTS: RefCell<Option<Vec<OsString>>> = const { RefCell::new(None) };
}

/// Parse the arguments supplied by the hierarchical `nsb-data` command.
pub fn parse_command_args<T: Parser>() -> T {
    COMMAND_ARGUMENTS.with(|stored| match stored.borrow().as_ref() {
        Some(arguments) => T::parse_from(arguments),
        None => T::parse(),
    })
}

/// Run one action with its command-local argument vector.
pub fn with_command_args<T>(arguments: Vec<OsString>, run: impl FnOnce() -> T) -> T {
    COMMAND_ARGUMENTS.with(|stored| {
        assert!(stored.borrow().is_none(), "nested nsb-data action dispatch");
        *stored.borrow_mut() = Some(arguments);
        let result = run();
        *stored.borrow_mut() = None;
        result
    })
}
