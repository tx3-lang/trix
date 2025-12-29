use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use askama::Template as _;
use bip39::Mnemonic;
use cryptoxide::{digest::Digest, sha2::Sha256};
use miette::{Context, IntoDiagnostic as _, Result, bail};

use crate::{
    config::{IdentityConfig, NetworkConfig, ProfileConfig, RandomKeyIdentityConfig, RootConfig},
    spawn::cshell::{CshellTomlTemplate, Provider, WalletInfoOutput},
};

fn generate_deterministic_mnemonic(input: &str) -> miette::Result<Mnemonic> {
    let mut hasher = Sha256::new();
    hasher.input(input.as_bytes());
    let hash = hasher.result_str();

    let entropy: [u8; 32] = hash[..32].as_bytes().try_into().unwrap();

    Mnemonic::from_entropy(&entropy).into_diagnostic()
}

fn setup_wallet_key(home: &Path, ident: &str) -> miette::Result<String> {
    let mnemonic = generate_deterministic_mnemonic(ident)?.to_string();

    let output = crate::spawn::cshell::wallet_create(home, ident, &mnemonic)?;

    let address = output
        .get("addresses")
        .context("missing 'addresses' field in cshell JSON output")?
        .get("testnet")
        .context("missing 'testnet' field in cshell 'addresses'")?
        .as_str()
        .unwrap();

    Ok(address.to_string())
}

fn provider_name(trix_profile: &str) -> String {
    format!("trix-{}", trix_profile)
}

pub struct WalletProxy {
    pub target_dir: PathBuf,
    pub addresses: HashMap<String, String>,
}

impl WalletProxy {
    pub fn info(&self, name: &str) -> miette::Result<WalletInfoOutput> {
        let output = crate::spawn::cshell::wallet_info(&self.target_dir, name)?;

        Ok(output)
    }

    pub fn explorer(&self, profile: &str) -> miette::Result<()> {
        let provider = provider_name(profile);

        let mut child = crate::spawn::cshell::explorer(&self.target_dir, &provider)?;

        let status = child
            .wait()
            .into_diagnostic()
            .context("failed to wait for cshell explorer")?;

        if !status.success() {
            bail!("cshell explorer exited with code: {}", status);
        }

        Ok(())
    }

    pub fn invoke_interactive(
        &self,
        tii_file: &Path,
        args: &serde_json::Value,
        profile: &str,
        skip_submit: bool,
    ) -> miette::Result<()> {
        let provider = provider_name(profile);

        crate::spawn::cshell::tx_invoke_interactive(
            &self.target_dir,
            &tii_file,
            Some(profile),
            None,
            &args,
            vec![],
            true,
            skip_submit,
            Some(&provider),
        )?;

        Ok(())
    }

    pub fn invoke_json(
        &self,
        tii_file: &Path,
        tx_template: &str,
        args: &serde_json::Value,
        signers: Vec<&str>,
        profile: &str,
    ) -> miette::Result<serde_json::Value> {
        let provider = provider_name(profile);

        let output = crate::spawn::cshell::tx_invoke_json(
            &self.target_dir,
            &tii_file,
            Some(profile),
            args,
            Some(tx_template),
            signers,
            true,
            false,
            Some(&provider),
        )?;

        Ok(output)
    }
}

fn define_provider(profile_name: &str, network: &NetworkConfig) -> Result<Provider> {
    Ok(Provider {
        name: provider_name(profile_name),
        u5c: network.u5c.clone(),
        trp: network.trp.clone(),
        is_testnet: network.is_testnet,
    })
}

pub fn setup(protocol: &RootConfig, profile: &ProfileConfig) -> miette::Result<WalletProxy> {
    let target_dir = crate::dirs::target_dir("cshell")?;

    let network = protocol.resolve_profile_network(&profile.name)?;

    let toml = CshellTomlTemplate {
        provider: define_provider(profile.name.as_str(), &network)?,
    };

    let toml = toml.render().into_diagnostic()?;

    let toml_path = target_dir.join("cshell.toml");

    std::fs::write(&toml_path, toml)
        .into_diagnostic()
        .context("writing cshell config")?;

    let mut addresses = HashMap::new();

    for (name, ident) in profile.identities.iter() {
        if let IdentityConfig::RandomKey(ident) = ident {
            let address = setup_wallet_key(&target_dir, &ident.name)?;
            addresses.insert(name.clone(), address);
        } else {
            bail!("only random key identities are supported");
        }
    }

    Ok(WalletProxy {
        target_dir,
        addresses,
    })
}
