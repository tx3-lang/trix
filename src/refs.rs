//! Canonical reference grammar for protocols and transactions.
//!
//! Every place trix asks the user to mention a protocol or a transaction —
//! CLI flags, `trix.toml` values, error messages — funnels through the
//! parsers in this module. See the grammar block below for the exact shape.
//!
//! ```text
//! protocol_ref ::= alias | registry_ref
//! alias        ::= IDENT
//! registry_ref ::= SCOPE "/" NAME [":" VERSION]
//!
//! tx_ref       ::= [protocol_ref "::"] TX_NAME
//!
//! IDENT, SCOPE, NAME, TX_NAME ::= [a-zA-Z_][a-zA-Z0-9_.-]*
//! VERSION                     ::= [a-zA-Z0-9_][a-zA-Z0-9_.-]*   (OCI tag rules)
//! ```

use std::fmt;
use std::str::FromStr;

use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error, Diagnostic)]
pub enum ParseError {
    #[error("empty reference")]
    Empty,

    #[error("invalid protocol reference '{0}': scope must contain at most one '/'")]
    MalformedScope(String),

    #[error("invalid protocol reference '{0}': empty scope")]
    EmptyScope(String),

    #[error("invalid protocol reference '{0}': empty name")]
    EmptyName(String),

    #[error("invalid protocol reference '{0}': empty version after ':'")]
    EmptyVersion(String),

    #[error("invalid identifier '{0}': must match [a-zA-Z_][a-zA-Z0-9_.-]*")]
    InvalidIdent(String),

    #[error("invalid OCI tag '{0}': must match [a-zA-Z0-9_][a-zA-Z0-9_.-]*")]
    InvalidVersion(String),

    #[error("invalid tx reference '{0}': '::' appears more than once")]
    TooManySeparators(String),

    #[error("invalid tx reference '{0}': empty tx name")]
    EmptyTx(String),

    #[error("'trix use' requires a full registry reference (e.g. acme/widget:0.1.0), got alias '{0}'")]
    AliasOnlyNotAllowed(String),
}

// ============================================================================
// ProtocolRef
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolRef {
    /// e.g. "widget" — resolved through the project's [dependencies] map or
    /// matched against the project's own protocol.name.
    Alias(String),

    /// e.g. "acme/widget" or "acme/widget:0.1.0".
    Registry {
        scope: String,
        name: String,
        version: Option<String>,
    },
}

impl ProtocolRef {
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        if s.is_empty() {
            return Err(ParseError::Empty);
        }

        if !s.contains('/') {
            validate_ident(s)?;
            return Ok(ProtocolRef::Alias(s.to_string()));
        }

        let (scope_name, version) = match s.split_once(':') {
            Some((sn, v)) => {
                if v.is_empty() {
                    return Err(ParseError::EmptyVersion(s.to_string()));
                }
                (sn, Some(v))
            }
            None => (s, None),
        };

        let mut parts = scope_name.split('/');
        let scope = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if parts.next().is_some() {
            return Err(ParseError::MalformedScope(s.to_string()));
        }
        if scope.is_empty() {
            return Err(ParseError::EmptyScope(s.to_string()));
        }
        if name.is_empty() {
            return Err(ParseError::EmptyName(s.to_string()));
        }
        validate_ident(scope)?;
        validate_ident(name)?;

        if let Some(v) = version {
            validate_oci_tag(v)?;
        }

        Ok(ProtocolRef::Registry {
            scope: scope.to_string(),
            name: name.to_string(),
            version: version.map(|v| v.to_string()),
        })
    }

    /// Clap value parser variant: rejects alias-only refs. Used by `trix use`
    /// because aliases don't carry version info.
    pub fn parse_registry(s: &str) -> Result<Self, ParseError> {
        let parsed = Self::parse(s)?;
        match parsed {
            ProtocolRef::Alias(a) => Err(ParseError::AliasOnlyNotAllowed(a)),
            r @ ProtocolRef::Registry { .. } => Ok(r),
        }
    }

    /// The short name carried by either variant. For `Alias` this is the
    /// alias itself; for `Registry` it's the `name` field.
    pub fn short_name(&self) -> &str {
        match self {
            ProtocolRef::Alias(a) => a,
            ProtocolRef::Registry { name, .. } => name,
        }
    }
}

impl fmt::Display for ProtocolRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolRef::Alias(a) => f.write_str(a),
            ProtocolRef::Registry {
                scope,
                name,
                version: None,
            } => write!(f, "{}/{}", scope, name),
            ProtocolRef::Registry {
                scope,
                name,
                version: Some(v),
            } => write!(f, "{}/{}:{}", scope, name, v),
        }
    }
}

impl FromStr for ProtocolRef {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for ProtocolRef {
    type Error = ParseError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<ProtocolRef> for String {
    fn from(r: ProtocolRef) -> Self {
        r.to_string()
    }
}

impl Serialize for ProtocolRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ProtocolRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        ProtocolRef::parse(&s).map_err(serde::de::Error::custom)
    }
}

// ============================================================================
// TxRef
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxRef {
    /// None = the project's own protocol.
    pub protocol: Option<ProtocolRef>,
    pub tx: String,
}

impl TxRef {
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        if s.is_empty() {
            return Err(ParseError::Empty);
        }

        // Split on the FIRST occurrence of "::". A version inside the
        // protocol part uses a single ":" and is safe.
        let (protocol_part, tx_name) = match s.split_once("::") {
            None => (None, s),
            Some((left, right)) => {
                if right.contains("::") {
                    return Err(ParseError::TooManySeparators(s.to_string()));
                }
                (Some(left), right)
            }
        };

        if tx_name.is_empty() {
            return Err(ParseError::EmptyTx(s.to_string()));
        }
        validate_ident(tx_name)?;

        let protocol = match protocol_part {
            Some(p) => Some(ProtocolRef::parse(p)?),
            None => None,
        };

        Ok(TxRef {
            protocol,
            tx: tx_name.to_string(),
        })
    }
}

impl fmt::Display for TxRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.protocol {
            None => f.write_str(&self.tx),
            Some(p) => write!(f, "{}::{}", p, self.tx),
        }
    }
}

impl FromStr for TxRef {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// ============================================================================
// Validation helpers
// ============================================================================

pub fn validate_ident(s: &str) -> Result<(), ParseError> {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return Err(ParseError::InvalidIdent(s.to_string()));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(ParseError::InvalidIdent(s.to_string()));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
            return Err(ParseError::InvalidIdent(s.to_string()));
        }
    }
    Ok(())
}

fn validate_oci_tag(s: &str) -> Result<(), ParseError> {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return Err(ParseError::InvalidVersion(s.to_string()));
    };
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return Err(ParseError::InvalidVersion(s.to_string()));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
            return Err(ParseError::InvalidVersion(s.to_string()));
        }
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alias() {
        let r = ProtocolRef::parse("widget").unwrap();
        assert_eq!(r, ProtocolRef::Alias("widget".into()));
    }

    #[test]
    fn parses_registry_without_version() {
        let r = ProtocolRef::parse("acme/widget").unwrap();
        assert_eq!(
            r,
            ProtocolRef::Registry {
                scope: "acme".into(),
                name: "widget".into(),
                version: None,
            }
        );
    }

    #[test]
    fn parses_registry_with_version() {
        let r = ProtocolRef::parse("acme/widget:0.1.0").unwrap();
        assert_eq!(
            r,
            ProtocolRef::Registry {
                scope: "acme".into(),
                name: "widget".into(),
                version: Some("0.1.0".into()),
            }
        );
    }

    #[test]
    fn rejects_double_slash() {
        assert!(matches!(
            ProtocolRef::parse("a/b/c"),
            Err(ParseError::MalformedScope(_))
        ));
    }

    #[test]
    fn rejects_empty_version() {
        assert!(matches!(
            ProtocolRef::parse("acme/widget:"),
            Err(ParseError::EmptyVersion(_))
        ));
    }

    #[test]
    fn parse_registry_rejects_alias_only() {
        assert!(matches!(
            ProtocolRef::parse_registry("widget"),
            Err(ParseError::AliasOnlyNotAllowed(_))
        ));
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "widget",
            "acme/widget",
            "acme/widget:0.1.0",
            "acme/widget:latest",
        ] {
            let r = ProtocolRef::parse(s).unwrap();
            assert_eq!(r.to_string(), s);
        }
    }

    #[test]
    fn tx_ref_bare() {
        let r = TxRef::parse("transfer").unwrap();
        assert_eq!(
            r,
            TxRef {
                protocol: None,
                tx: "transfer".into()
            }
        );
    }

    #[test]
    fn tx_ref_alias_qualified() {
        let r = TxRef::parse("widget::transfer").unwrap();
        assert_eq!(
            r,
            TxRef {
                protocol: Some(ProtocolRef::Alias("widget".into())),
                tx: "transfer".into(),
            }
        );
    }

    #[test]
    fn tx_ref_full_qualified() {
        let r = TxRef::parse("acme/widget:0.1.0::transfer").unwrap();
        assert_eq!(
            r,
            TxRef {
                protocol: Some(ProtocolRef::Registry {
                    scope: "acme".into(),
                    name: "widget".into(),
                    version: Some("0.1.0".into()),
                }),
                tx: "transfer".into(),
            }
        );
    }

    #[test]
    fn tx_ref_rejects_extra_separator() {
        assert!(matches!(
            TxRef::parse("a::b::c"),
            Err(ParseError::TooManySeparators(_))
        ));
    }

    #[test]
    fn tx_ref_display_round_trips() {
        for s in [
            "transfer",
            "widget::transfer",
            "acme/widget::transfer",
            "acme/widget:0.1.0::transfer",
        ] {
            let r = TxRef::parse(s).unwrap();
            assert_eq!(r.to_string(), s);
        }
    }
}
