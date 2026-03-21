pub mod build;
pub mod check;
pub mod devnet;
pub mod expect;
pub mod explore;
pub mod identities;
pub mod init;
pub mod inspect;
pub mod invoke;
pub mod profile;
pub mod publish;
pub mod telemetry;
pub mod test;

#[cfg(feature = "unstable")]
pub mod codegen;

#[cfg(not(feature = "unstable"))]
pub mod codegen_legacy;

pub use codegen_legacy as codegen;
