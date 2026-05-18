//! Resolves a parsed reference (`refs::ProtocolRef` / `TxRef`) to a concrete
//! artifact: either the project's own protocol or a declared dependency.
//! This is dependency-domain logic — it queries `config.dependencies` — so
//! it lives here rather than in the pure `refs` grammar module.

use miette::Diagnostic;
use thiserror::Error;

use crate::config::{DependencyEntry, RootConfig};
use crate::refs::{ProtocolRef, TxRef};

#[derive(Debug, Error, Diagnostic)]
pub enum ResolveError {
    #[error("no protocol named '{0}' (not the project, not a dependency alias)")]
    UnknownAlias(String),

    #[error("no dependency matches '{0}' — declare it with 'trix use'")]
    UnknownRegistryRef(String),

    #[error("dependency '{alias}' matches '{scope}/{name}' but at version '{have}', not '{want}'")]
    VersionMismatch {
        alias: String,
        scope: String,
        name: String,
        want: String,
        have: String,
    },
}

#[derive(Debug, Clone)]
pub enum ResolvedProtocol<'a> {
    /// The project's authored protocol (`config.protocol.main`).
    Project,
    /// A dep declared in `[dependencies]` and resolved to a cached artifact.
    Dependency(&'a DependencyEntry),
}

pub struct Resolver<'a> {
    config: &'a RootConfig,
}

impl<'a> Resolver<'a> {
    pub fn new(config: &'a RootConfig) -> Self {
        Self { config }
    }

    pub fn resolve_protocol(
        &self,
        r: &ProtocolRef,
    ) -> Result<ResolvedProtocol<'a>, ResolveError> {
        match r {
            ProtocolRef::Alias(a) => {
                if a == &self.config.protocol.name {
                    return Ok(ResolvedProtocol::Project);
                }
                if let Some(entry) = self.config.dependencies.get(a) {
                    return Ok(ResolvedProtocol::Dependency(entry));
                }
                Err(ResolveError::UnknownAlias(a.clone()))
            }
            ProtocolRef::Registry {
                scope,
                name,
                version,
            } => {
                let candidate = self.config.dependencies.values().find(|d| {
                    if let ProtocolRef::Registry {
                        scope: ds, name: dn, ..
                    } = &d.reference
                    {
                        ds == scope && dn == name
                    } else {
                        false
                    }
                });
                let Some(entry) = candidate else {
                    return Err(ResolveError::UnknownRegistryRef(r.to_string()));
                };
                let have_ver = match &entry.reference {
                    ProtocolRef::Registry {
                        version: Some(v), ..
                    } => Some(v),
                    _ => None,
                };
                if let (Some(want), Some(have)) = (version.as_ref(), have_ver)
                    && want != have
                {
                    return Err(ResolveError::VersionMismatch {
                        alias: entry.alias.clone(),
                        scope: scope.clone(),
                        name: name.clone(),
                        want: want.clone(),
                        have: have.clone(),
                    });
                }
                Ok(ResolvedProtocol::Dependency(entry))
            }
        }
    }

    pub fn resolve_tx<'r>(
        &self,
        r: &'r TxRef,
    ) -> Result<(ResolvedProtocol<'a>, &'r str), ResolveError> {
        let protocol = match &r.protocol {
            None => ResolvedProtocol::Project,
            Some(p) => self.resolve_protocol(p)?,
        };
        Ok((protocol, r.tx.as_str()))
    }
}
