//! Horizon Open Creative players on a vanilla or Paper 26.3 server: a
//! Minecraft protocol relay that speaks the open bundle's numbering to its
//! players and vanilla's to the server (docs/open-creative.md, "Joining a
//! Java server through the relay").
#![forbid(unsafe_code)]
pub mod policy;
pub mod recipes;
pub mod relay;
pub mod rules;
pub mod tables;
pub mod translate;
pub mod vanilla;

#[cfg(test)]
mod tests;
