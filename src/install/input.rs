use crate::{
    cli::Install,
    config::{
        Configuration, Definition, Server,
        schema::{Runtime, Stdio},
    },
    error::Error,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};

pub(super) fn parse(install: &Install) -> Result<Server, Error> {
    let server: Server = if let Some(path) = &install.source.config {
        let file: std::fs::File = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map(std::fs::File::from)
        .map_err(crate::storage::io_error)?;
        if !file.metadata().map_err(crate::storage::io_error)?.is_file() {
            return Err(super::invalid());
        }
        let mut bytes: Vec<u8> = Vec::new();
        file.take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(crate::storage::io_error)?;
        if bytes.len() > 4 * 1024 * 1024 || !install.args.is_empty() {
            return Err(super::invalid());
        }
        let value: serde_json::Value = crate::json::parse(&bytes).map_err(|_| super::invalid())?;
        serde_json::from_value(value).map_err(|_| super::invalid())?
    } else {
        let (runtime, package): (Runtime, String) = match (
            &install.source.npx,
            &install.source.uvx,
            &install.source.docker,
        ) {
            (Some(package), None, None) => (Runtime::Npx, package.clone()),
            (None, Some(package), None) => (Runtime::Uvx, package.clone()),
            (None, None, Some(package)) => (Runtime::Docker, package.clone()),
            _ => return Err(super::invalid()),
        };
        Server {
            enabled: true,
            disabled_tools: BTreeSet::new(),
            definition: Definition::Local {
                runtime,
                package,
                transport: Stdio::Stdio,
                args: install.args.clone(),
                env: BTreeMap::new(),
                cwd: None,
            },
        }
    };
    validate(&install.server, &server)?;
    Ok(server)
}

pub(crate) fn validate(name: &str, server: &Server) -> Result<(), Error> {
    let config: Configuration = Configuration {
        servers: BTreeMap::from([(name.to_owned(), server.clone())]),
        ..Configuration::default()
    };
    config.validate()?;
    validate_remote(&server.definition)
}

fn validate_remote(definition: &Definition) -> Result<(), Error> {
    let Definition::Remote {
        url,
        headers,
        authentication,
        ..
    }: &Definition = definition
    else {
        return Ok(());
    };
    let endpoint: url::Url = crate::config::schema::endpoint(url)?;
    crate::mcp::validate_http_endpoint(&endpoint)?;
    if (!headers.is_empty()
        || matches!(authentication, crate::config::schema::Authentication::Oauth))
        && endpoint.scheme() != "https"
    {
        return Err(super::invalid());
    }
    if matches!(authentication, crate::config::schema::Authentication::Oauth) {
        let _: crate::auth::Provider =
            crate::auth::Provider::new(endpoint).map_err(|_| super::invalid())?;
    }
    let mut names: BTreeSet<String> = BTreeSet::new();
    for name in headers.keys() {
        let name: String = name.to_ascii_lowercase();
        if !names.insert(name.clone())
            || (name != "authorization" && crate::mcp::reserved_http_header(&name))
        {
            return Err(super::invalid());
        }
        if name == "authorization"
            && matches!(authentication, crate::config::schema::Authentication::Oauth)
        {
            return Err(super::invalid());
        }
    }
    Ok(())
}
