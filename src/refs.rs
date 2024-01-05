//! Refs are text files containing a hexadecimal representation of an objects’s hash, encoded in ASCII.
//!
//! Refs can also refer to another reference, and thus only indirectly to an objects.

mod branch;
pub mod tag;
