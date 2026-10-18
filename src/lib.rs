//! Editable photobooks compiled with the official embedded Typst compiler.

mod booklet;
mod compile;
mod engine;
mod generate;

pub use booklet::BookletStats;
pub use compile::{Compilation, compile};
pub use generate::{Generation, generate};
