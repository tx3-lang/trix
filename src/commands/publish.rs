use crate::config::Config;
use clap::Args as ClapArgs;
use miette::IntoDiagnostic as _;
use serde::{Deserialize, Serialize};

const MARKDOWN_MEDIA_TYPE: &str = "text/markdown";
const PROTOCOL_MEDIA_TYPE: &str = "application/tx3";

#[derive(ClapArgs)]
/// Arguments for the publish command (UNSTABLE - experimental feature)
pub struct Args {}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageMetadata {
    pub name: String,
    pub scope: String,
    pub published_date: i64,
    pub repository_url: Option<String>,
    pub description: Option<String>,
}

fn get_oci_client(config: &Config) -> oci_client::Client {
    let registry_url = config.registry.clone().unwrap().url;
    let registry_protocol = registry_url.split("://").next().unwrap();

    let client_config = oci_client::client::ClientConfig {
        protocol: if registry_protocol == "http" {
            oci_client::client::ClientProtocol::Http
        } else {
            oci_client::client::ClientProtocol::Https
        },
        ..Default::default()
    };

    return oci_client::Client::new(client_config);
}

fn get_oci_reference(config: &Config) -> Result<oci_client::Reference, oci_client::ParseError> {
    let registry_url = config.registry.clone().unwrap().url;
    let registry_host = registry_url.split("://").collect::<Vec<_>>().pop().unwrap();
    oci_client::Reference::try_from(format!("{}/{}/{}:{}",
        registry_host,
        config.protocol.scope.clone().unwrap(),
        config.protocol.name.clone(),
        config.protocol.version.clone()
    ))
}

fn get_image_url(config: &Config) -> String {
    let registry_url = config.registry.clone().unwrap().url;
    format!("{}/image/{}%2F{}/tag/{}",
        registry_url,
        config.protocol.scope.clone().unwrap(),
        config.protocol.name.clone(),
        config.protocol.version.clone()
    )
}

pub fn run(_args: Args, config: &Config) -> miette::Result<()> {
    #[cfg(feature = "unstable")]
    {
        _run(_args, config)
    }
    #[cfg(not(feature = "unstable"))]
    {
        Err(miette::miette!("The publish command is currently unstable and requires the `unstable` feature to be enabled."))
    }
}

pub fn _run(_args: Args, config: &Config) -> miette::Result<()> {    
    if config.protocol.scope.is_none() {
        return Err(miette::miette!("No scope found in trix.toml"));
    }

    let scope = config.protocol.scope.clone().unwrap();
    let name = config.protocol.name.clone();
    let published_date = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    let protocol = std::fs::read_to_string(config.protocol.main.clone()).into_diagnostic()?;

    let mut image_layers = vec![
        oci_client::client::ImageLayer::new(
            protocol.as_bytes().to_vec(),
            PROTOCOL_MEDIA_TYPE.to_string(),
            None,
        )
    ];
    
    if config.protocol.readme.is_some() {
        let readme = std::fs::read_to_string(config.protocol.readme.clone().unwrap()).into_diagnostic()?;
        image_layers.push(
            oci_client::client::ImageLayer::new(
                readme.as_bytes().to_vec(),
                MARKDOWN_MEDIA_TYPE.to_string(),
                None,
            )
        );
    }

    let image_config = oci_client::client::Config {
        data: serde_json::to_vec(&ImageMetadata {
            name: name.clone(),
            scope: scope.clone(),
            published_date,
            repository_url: None,
            description: config.protocol.description.clone(),
        }).into_diagnostic()?,
        media_type: oci_client::manifest::IMAGE_CONFIG_MEDIA_TYPE.to_string(),
        annotations: None,
    };

    let image_manifest = oci_client::manifest::OciImageManifest::build(
        &image_layers,
        &image_config,
        Some(std::collections::BTreeMap::from([
            (
                "org.opencontainers.image.created".to_string(),
                chrono::DateTime::from_timestamp(published_date, 0).unwrap().to_rfc3339()
            ),
            ("org.opencontainers.image.vendor".to_string(), scope.clone()),
            ("org.opencontainers.image.title".to_string(), name.clone()),
            ("org.opencontainers.image.version".to_string(), config.protocol.version.to_string()),
            ("org.opencontainers.image.description".to_string(), config.protocol.description.clone().unwrap_or_default()),
        ]))
    );

    let image_reference = get_oci_reference(config).into_diagnostic()?;
    let oci_client = get_oci_client(config);
    let digest = futures::executor::block_on(
        oci_client.push(
            &image_reference,
            &image_layers,
            image_config,
            &oci_client::secrets::RegistryAuth::Anonymous,
            Some(image_manifest)
        )
    ).into_diagnostic()?;

    println!("Image pushed successfully!");
    println!("Image URL: {}", get_image_url(config));
    println!("Manifest URL: {}", digest.manifest_url);

    Ok(())
}
