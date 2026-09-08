use crate::config::{
    Definition, Project,
    schema::{Authentication, Runtime, endpoint},
};
use crate::deadline::Timeouts;
use crate::error::Error;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use url::Url;

#[derive(Serialize)]
pub struct Preview<'a> {
    root: &'a Path,
    configuration_digest: &'a str,
    timeouts: &'a Option<Timeouts>,
    servers: BTreeMap<&'a str, Effect<'a>>,
    redaction: &'static str,
}

#[derive(Serialize)]
struct Effect<'a> {
    enabled: bool,
    disabled_tools: &'a std::collections::BTreeSet<String>,
    definition: Execution<'a>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Execution<'a> {
    Local {
        runtime: Runtime,
        package: String,
        argument_count: usize,
        environment_names: Vec<&'a str>,
        cwd: Option<&'a Path>,
    },
    Remote {
        origin: String,
        endpoint_digest: String,
        header_names: Vec<&'a str>,
        authentication: Authentication,
    },
}

impl<'a> Preview<'a> {
    pub fn new(project: &'a Project) -> Result<Self, Error> {
        let mut servers: BTreeMap<&str, Effect<'_>> = BTreeMap::new();
        for (name, server) in &project.configuration().servers {
            let definition: Execution<'_> = match &server.definition {
                Definition::Local {
                    runtime,
                    package,
                    args,
                    env,
                    cwd,
                    ..
                } => Execution::Local {
                    runtime: *runtime,
                    package: safe_package(package),
                    argument_count: args.len(),
                    environment_names: env.keys().map(String::as_str).collect(),
                    cwd: cwd.as_deref(),
                },
                Definition::Remote {
                    url,
                    headers,
                    authentication,
                    ..
                } => Execution::Remote {
                    origin: endpoint(url)?.origin().ascii_serialization(),
                    endpoint_digest: crate::config::digest(url.as_bytes()),
                    header_names: headers.keys().map(String::as_str).collect(),
                    authentication: *authentication,
                },
            };
            servers.insert(
                name,
                Effect {
                    enabled: server.enabled,
                    disabled_tools: &server.disabled_tools,
                    definition,
                },
            );
        }
        Ok(Self {
            root: project.root(),
            configuration_digest: project.digest(),
            timeouts: &project.configuration().timeouts,
            servers,
            redaction: "Arguments, secret references, URL paths/queries and URL package details are hidden. Review the local config before approving; its exact bytes are bound to this digest.",
        })
    }
}

fn safe_package(package: &str) -> String {
    match Url::parse(package) {
        Ok(url) => format!("{} [details redacted]", url.origin().ascii_serialization()),
        Err(_) if package.contains('@') && package.contains(':') => {
            "[package reference redacted]".to_owned()
        }
        Err(_) => package.to_owned(),
    }
}
