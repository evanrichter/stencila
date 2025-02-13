mod assemble;
mod compile;
mod document;
mod documents;
mod executable;
mod execute;
mod messages;
mod utils;

pub use crate::documents::DOCUMENTS;
pub use crate::messages::When;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(test)]
mod tests;
