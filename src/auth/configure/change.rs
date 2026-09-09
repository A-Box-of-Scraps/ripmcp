use crate::{
    cli::AuthConfigure,
    config::{
        Definition, Server,
        authentication::{ClientAuthMethod, OAuthClient},
        schema::{Authentication, SecretReference},
    },
    error::{Error, ErrorKind},
};

pub(super) fn server(
    args: &AuthConfigure,
    original: &Server,
    secret: &SecretReference,
) -> Result<Server, Error> {
    let mut server: Server = original.clone();
    let Definition::Remote {
        url,
        headers,
        authentication,
        bearer,
        oauth_client,
        ..
    }: &mut Definition = &mut server.definition
    else {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "auth configure requires a remote server; local server credentials are supplied through its environment",
        ));
    };
    if crate::config::schema::endpoint(url)?.scheme() != "https" {
        return Err(Error::new(
            ErrorKind::Configuration,
            "credentials require an HTTPS MCP endpoint",
        ));
    }
    *bearer = None;
    *oauth_client = None;
    if let Some(client_id) = &args.source.oauth_client_id {
        headers.retain(|name, _| !name.eq_ignore_ascii_case("authorization"));
        *authentication = Authentication::Oauth;
        *oauth_client = Some(client(args, client_id, secret)?);
    } else if let Some(header) = &args.source.header {
        if crate::mcp::reserved_http_header(&header.to_ascii_lowercase())
            || header.eq_ignore_ascii_case("authorization")
        {
            return Err(Error::new(
                ErrorKind::Usage,
                "reserved credential header; use --bearer or --bearer-env for Authorization",
            ));
        }
        headers.retain(|name, _| !name.eq_ignore_ascii_case(header));
        headers.insert(
            header.clone(),
            reference(args.header_env.as_deref(), secret),
        );
        *authentication = Authentication::None;
    } else {
        headers.retain(|name, _| !name.eq_ignore_ascii_case("authorization"));
        *authentication = Authentication::Bearer;
        *bearer = Some(reference(args.source.bearer_env.as_deref(), secret));
    }
    crate::install::input::validate(&args.server, &server)?;
    Ok(server)
}

fn reference(environment: Option<&str>, secret: &SecretReference) -> SecretReference {
    environment.map_or_else(
        || secret.clone(),
        |name| SecretReference::Environment(name.to_owned()),
    )
}

fn client(
    args: &AuthConfigure,
    client_id: &str,
    secret: &SecretReference,
) -> Result<OAuthClient, Error> {
    let client_secret: Option<SecretReference> = if args.client_secret {
        Some(secret.clone())
    } else {
        args.client_secret_env
            .as_ref()
            .map(|name| SecretReference::Environment(name.clone()))
    };
    let method: ClientAuthMethod =
        args.token_endpoint_auth_method
            .unwrap_or(if client_secret.is_some() {
                ClientAuthMethod::ClientSecretPost
            } else {
                ClientAuthMethod::None
            });
    Ok(OAuthClient {
        issuer: args
            .issuer
            .clone()
            .ok_or_else(|| Error::new(ErrorKind::Usage, "OAuth registration requires --issuer"))?,
        client_id: client_id.to_owned(),
        client_secret,
        token_endpoint_auth_method: method,
        scopes: args.scopes.clone(),
    })
}
