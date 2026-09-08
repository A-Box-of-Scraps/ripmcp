use crate::deadline::Timeouts;
use crate::error::{Error, ErrorKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use url::Url;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Version;

impl TryFrom<u32> for Version {
    type Error = &'static str;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 1 {
            Ok(Self)
        } else {
            Err("unsupported schema_version")
        }
    }
}
impl From<Version> for u32 {
    fn from(_: Version) -> Self {
        1
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub schema_version: Version,
    #[serde(default, deserialize_with = "super::unique::map")]
    pub servers: BTreeMap<String, Server>,
    #[serde(
        default,
        deserialize_with = "present_timeouts",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeouts: Option<Timeouts>,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            schema_version: Version,
            servers: BTreeMap::new(),
            timeouts: None,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub disabled_tools: BTreeSet<String>,
    pub definition: Definition,
}

fn enabled() -> bool {
    true
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Definition {
    Local {
        runtime: Runtime,
        package: String,
        transport: Stdio,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, deserialize_with = "super::unique::map")]
        env: BTreeMap<String, SecretReference>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<PathBuf>,
    },
    Remote {
        url: String,
        transport: StreamableHttp,
        #[serde(default, deserialize_with = "super::unique::map")]
        headers: BTreeMap<String, SecretReference>,
        #[serde(default)]
        authentication: Authentication,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    Npx,
    Uvx,
    Docker,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stdio {
    Stdio,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamableHttp {
    StreamableHttp,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authentication {
    #[default]
    None,
    Oauth,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(try_from = "ReferenceWire", into = "ReferenceWire")]
pub enum SecretReference {
    Environment(String),
    Keyring(String),
}

#[derive(Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
enum ReferenceWire {
    Environment { env: String },
    Keyring { keyring: String },
}

impl TryFrom<ReferenceWire> for SecretReference {
    type Error = &'static str;
    fn try_from(value: ReferenceWire) -> Result<Self, Self::Error> {
        match value {
            ReferenceWire::Environment { env } if environment_name(&env) => {
                Ok(Self::Environment(env))
            }
            ReferenceWire::Keyring { keyring } if opaque_key(&keyring) => {
                Ok(Self::Keyring(keyring))
            }
            _ => Err("invalid secret reference"),
        }
    }
}
impl From<SecretReference> for ReferenceWire {
    fn from(value: SecretReference) -> Self {
        match value {
            SecretReference::Environment(env) => Self::Environment { env },
            SecretReference::Keyring(keyring) => Self::Keyring { keyring },
        }
    }
}

fn opaque_key(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
}

pub fn environment_name(value: &str) -> bool {
    let mut bytes: std::str::Bytes<'_> = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

impl Configuration {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut deserializer: serde_json::Deserializer<serde_json::de::SliceRead<'_>> =
            serde_json::Deserializer::from_slice(bytes);
        let config: Self = serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|error| Error::field(&error.path().to_string()))?;
        deserializer.end().map_err(|_| Error::field("document"))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Error> {
        for (name, server) in &self.servers {
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err(Error::field("servers.name"));
            }
            if server
                .disabled_tools
                .iter()
                .any(|name| name.is_empty() || name.chars().any(char::is_control))
            {
                return Err(Error::field("servers.disabled_tools"));
            }
            server.definition.validate()?;
        }
        Ok(())
    }
}

impl Definition {
    pub fn validate(&self) -> Result<(), Error> {
        self.validate_references()?;
        match self {
            Self::Local {
                package,
                args,
                env,
                cwd,
                ..
            } => {
                if package.is_empty()
                    || package.starts_with('-')
                    || package.chars().any(char::is_control)
                {
                    return Err(Error::field("servers.definition.package"));
                }
                if args.iter().any(|arg| arg.contains('\0')) {
                    return Err(Error::field("servers.definition.args"));
                }
                if env.keys().any(|name| !environment_name(name)) {
                    return Err(Error::field("servers.definition.env"));
                }
                if cwd.as_ref().is_some_and(|path| {
                    !path.is_absolute() || path.as_os_str().as_encoded_bytes().contains(&0)
                }) {
                    return Err(Error::field("servers.definition.cwd"));
                }
            }
            Self::Remote { url, headers, .. } => {
                endpoint(url)?;
                if headers.keys().any(|name| !header_name(name)) {
                    return Err(Error::field("servers.definition.headers"));
                }
            }
        }
        Ok(())
    }
}

impl Definition {
    fn validate_references(&self) -> Result<(), Error> {
        let references: &BTreeMap<String, SecretReference> = match self {
            Self::Local { env, .. } => env,
            Self::Remote { headers, .. } => headers,
        };
        for reference in references.values() {
            let valid = match reference {
                SecretReference::Environment(name) => environment_name(name),
                SecretReference::Keyring(id) => opaque_key(id),
            };
            if !valid {
                return Err(Error::field("servers.definition.secret_reference"));
            }
        }
        Ok(())
    }
}

pub fn endpoint(value: &str) -> Result<Url, Error> {
    let url: Url = Url::parse(value).map_err(|_| Error::field("servers.definition.url"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || value.chars().any(char::is_control)
    {
        return Err(Error::new(
            ErrorKind::Configuration,
            "invalid servers.definition.url: expected HTTP endpoint without userinfo or fragment",
        ));
    }
    Ok(url)
}

fn header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn present_timeouts<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Timeouts>, D::Error> {
    Timeouts::deserialize(deserializer).map(Some)
}
