#![warn(
	missing_copy_implementations,
	missing_debug_implementations,
	missing_docs,
	trivial_numeric_casts,
	unused_extern_crates,
	unused_import_braces,
	unused_qualifications,
	unused_results,
	clippy::pedantic
)] // from https://github.com/rust-unofficial/patterns/blob/master/anti_patterns/deny-warnings.md
#![allow(
	clippy::inline_always,
	clippy::doc_markdown,
	clippy::trivially_copy_pass_by_ref
)]

extern crate metatype;
extern crate relative;
#[macro_use]
extern crate serde_derive;
extern crate bincode;
extern crate serde_json;

use relative::*;

#[path = "../src/tests.rs"]
mod tests;
