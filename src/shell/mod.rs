// Most of the code in this module has been lifted from `cargo`'s `shell.rs` module in order
// to match the output style of `cargo` as closely as possible.
mod hostname;
mod shell_;

pub use shell_::*;
