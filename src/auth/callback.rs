use super::{REDIRECT_URI, invalid, metadata::Profile, random, store::StoreFuture};
use crate::{
    error::{Error, ErrorKind},
    mcp::Operation,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use url::Url;

pub(super) trait Browser: Send + Sync {
    fn open<'a>(&'a self, url: &'a Url, operation: &'a Operation) -> StoreFuture<'a, ()>;
}

pub(super) struct SystemBrowser;

impl Browser for SystemBrowser {
    fn open<'a>(&'a self, url: &'a Url, operation: &'a Operation) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            {
                let mut stderr: std::io::StderrLock<'_> = std::io::stderr().lock();
                writeln!(stderr, "Open this URL in a browser on this machine:\n{url}\nWaiting for authorization; press Ctrl-C to cancel. Remote-headless redirects and device flow are not supported.")
                    .map_err(crate::storage::io_error)?;
                stderr.flush().map_err(crate::storage::io_error)?;
            }
            let launched = if std::env::var_os("DISPLAY").is_some()
                || std::env::var_os("WAYLAND_DISPLAY").is_some()
            {
                launch(url, operation).await?
            } else {
                false
            };
            if !launched {
                eprintln!(
                    "Browser launch unavailable; open the URL above manually. The local callback is still listening."
                );
            }
            Ok(())
        })
    }
}

async fn launch(url: &Url, operation: &Operation) -> Result<bool, Error> {
    let mut command: tokio::process::Command = tokio::process::Command::new("xdg-open");
    command
        .arg(url.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    launch_command(command, operation).await
}

async fn launch_command(
    mut command: tokio::process::Command,
    operation: &Operation,
) -> Result<bool, Error> {
    let Ok(mut child): Result<tokio::process::Child, std::io::Error> = command.spawn() else {
        return Ok(false);
    };
    operation
        .run(async {
            match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
                Ok(Ok(status)) => Ok(status.success()),
                _ => {
                    let _ = child.kill().await;
                    Ok(false)
                }
            }
        })
        .await
}

pub(super) struct Callback {
    listener: TcpListener,
    pub redirect: String,
    pub verifier: String,
    state: String,
}

impl Callback {
    pub async fn bind() -> Result<Self, Error> {
        Self::bind_address("127.0.0.1:42813").await
    }

    async fn bind_address(address: &str) -> Result<Self, Error> {
        let listener: TcpListener = TcpListener::bind(address).await.map_err(|_| Error::new(
            ErrorKind::Authentication, "OAuth loopback callback port 42813 is unavailable; close the conflicting listener and retry"))?;
        Self::new(listener, REDIRECT_URI.to_owned())
    }

    pub(super) fn new(listener: TcpListener, redirect: String) -> Result<Self, Error> {
        Ok(Self {
            listener,
            redirect,
            verifier: random()?,
            state: random()?,
        })
    }

    pub fn authorization_url(&self, profile: &Profile, client_id: &str) -> Result<Url, Error> {
        let mut url: Url =
            Url::parse(&profile.authorization.authorization_endpoint).map_err(|_| invalid())?;
        if url.query_pairs().any(|(name, _)| {
            matches!(
                name.as_ref(),
                "client_id"
                    | "redirect_uri"
                    | "response_type"
                    | "state"
                    | "code_challenge"
                    | "code_challenge_method"
                    | "resource"
                    | "scope"
            )
        }) {
            return Err(invalid());
        }
        let challenge: String = URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()));
        url.query_pairs_mut().extend_pairs([
            ("client_id", client_id),
            ("redirect_uri", &self.redirect),
            ("response_type", "code"),
            ("state", &self.state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("resource", &profile.resource),
        ]);
        if let Some(scope) = &profile.scope {
            url.query_pairs_mut().append_pair("scope", scope);
        }
        Ok(url)
    }

    pub async fn receive(&self, profile: &Profile, operation: &Operation) -> Result<String, Error> {
        operation.run(self.wait(profile)).await
    }

    async fn wait(&self, profile: &Profile) -> Result<String, Error> {
        loop {
            let (mut stream, peer): (TcpStream, std::net::SocketAddr) =
                self.listener.accept().await.map_err(|_| invalid())?;
            if !peer.ip().is_loopback() {
                return Err(invalid());
            }
            let request: Result<Result<String, Error>, tokio::time::error::Elapsed> =
                tokio::time::timeout(Duration::from_secs(2), read_request(&mut stream)).await;
            let Ok(Ok(request)): Result<Result<String, Error>, tokio::time::error::Elapsed> =
                request
            else {
                continue;
            };
            let result: Result<Option<String>, Error> = self.response(&request, profile);
            let status: &str = match &result {
                Ok(Some(_)) => "200 OK",
                Ok(None) => "404 Not Found",
                Err(_) => "400 Bad Request",
            };
            let response: String = format!(
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n"
            );
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                stream.write_all(response.as_bytes()),
            )
            .await;
            if let Some(code) = result? {
                return Ok(code);
            }
        }
    }

    fn response(&self, request: &str, profile: &Profile) -> Result<Option<String>, Error> {
        let target: &str = target(request, &self.redirect)?;
        let Some(query): Option<&str> = target.strip_prefix("/oauth/callback?") else {
            return Ok(None);
        };
        let fields: BTreeMap<String, String> = parameters(query)?;
        if fields.get("state") != Some(&self.state) {
            return Err(invalid());
        }
        match fields.get("iss") {
            Some(issuer) if issuer != &profile.authorization.issuer => return Err(invalid()),
            None if profile
                .authorization
                .authorization_response_iss_parameter_supported
                == Some(true) =>
            {
                return Err(invalid());
            }
            _ => (),
        }
        if fields.contains_key("error") {
            if fields.contains_key("code") {
                return Err(invalid());
            }
            return Err(Error::new(
                ErrorKind::Authentication,
                "OAuth authorization was denied or failed",
            ));
        }
        let code: String = fields
            .get("code")
            .filter(|code| !code.is_empty() && code.len() <= 8192)
            .ok_or_else(invalid)?
            .clone();
        Ok(Some(code))
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<String, Error> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut buffer: [u8; 1024] = [0; 1024];
    while bytes.len() <= 16384 {
        let count = stream.read(&mut buffer).await.map_err(|_| invalid())?;
        if count == 0 {
            return Err(invalid());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > 16384 {
            return Err(invalid());
        }
        if bytes.ends_with(b"\r\n\r\n") {
            return String::from_utf8(bytes).map_err(|_| invalid());
        }
    }
    Err(invalid())
}

fn target<'a>(request: &'a str, redirect: &str) -> Result<&'a str, Error> {
    let mut lines: std::str::Split<'_, &str> = request.split("\r\n");
    let words: Vec<&str> = lines.next().ok_or_else(invalid)?.split(' ').collect();
    if words.len() != 3 || words[0] != "GET" || words[2] != "HTTP/1.1" {
        return Err(invalid());
    }
    let mut hosts: Vec<&str> = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value): (&str, &str) = line.split_once(':').ok_or_else(invalid)?;
        if key.eq_ignore_ascii_case("host") {
            hosts.push(value.trim());
        }
        if key.eq_ignore_ascii_case("transfer-encoding")
            || key.eq_ignore_ascii_case("content-length")
        {
            return Err(invalid());
        }
    }
    let url: Url = Url::parse(redirect).map_err(|_| invalid())?;
    let expected: String = format!("127.0.0.1:{}", url.port().ok_or_else(invalid)?);
    if hosts != [expected.as_str()] {
        return Err(invalid());
    }
    Ok(words[1])
}

fn parameters(query: &str) -> Result<BTreeMap<String, String>, Error> {
    if query.contains('#') {
        return Err(invalid());
    }
    for (index, byte) in query.bytes().enumerate() {
        if byte == b'%'
            && !query
                .as_bytes()
                .get(index + 1..index + 3)
                .is_some_and(|bytes| bytes.iter().all(u8::is_ascii_hexdigit))
        {
            return Err(invalid());
        }
    }
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if value.contains('\u{fffd}')
            || value.chars().any(char::is_control)
            || fields
                .insert(key.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::{Callback, parameters, target};
    use crate::auth::{
        callback::Browser,
        metadata::Profile,
        tests::support::{Fixture, TestBrowser, callback, operation},
    };
    use std::collections::BTreeMap;
    use url::Url;

    #[test]
    fn callback_parameters_reject_duplicates_bad_encoding_and_fragments() {
        for query in [
            "state=a&state=b",
            "state=a&%73tate=b",
            "code=%GG",
            "code=%FF",
            "code=%0a",
            "code=a#fragment",
            "code=%",
        ] {
            assert!(parameters(query).is_err());
        }
        let parameters: BTreeMap<String, String> = parameters("code=a%2Bb&state=xyz").unwrap();
        assert_eq!(parameters["code"], "a+b");
    }

    #[test]
    fn callback_http_validates_method_host_and_framing() {
        let redirect: &str = "http://127.0.0.1:42813/oauth/callback";
        for request in [
            "POST /oauth/callback?code=x HTTP/1.1\r\nHost: 127.0.0.1:42813\r\n\r\n",
            "GET /oauth/callback?code=x HTTP/1.1\r\nHost: attacker.example\r\n\r\n",
            "GET /oauth/callback?code=x HTTP/1.1\r\nHost: 127.0.0.1:42813\r\nHost: 127.0.0.1:42813\r\n\r\n",
            "GET /oauth/callback?code=x HTTP/1.1\r\nHost: 127.0.0.1:42813\r\nTransfer-Encoding: chunked\r\n\r\n",
        ] {
            assert!(target(request, redirect).is_err());
        }
    }

    #[tokio::test]
    async fn occupied_callback_port_is_an_error_not_an_alternate_redirect() {
        let listener: tokio::net::TcpListener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        assert!(
            Callback::bind_address(&listener.local_addr().unwrap().to_string())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn unavailable_browser_does_not_close_manual_callback() {
        let fixture: Fixture = Fixture::new().await;
        let profile: Profile = fixture
            .provider()
            .network
            .discover(&fixture.endpoint, &operation())
            .await
            .unwrap();
        let callback: Callback = callback().await;
        let command: tokio::process::Command =
            tokio::process::Command::new(fixture.root.path().join("missing-browser"));
        assert!(!super::launch_command(command, &operation()).await.unwrap());
        let browser: TestBrowser = TestBrowser::new(fixture.issuer());
        let url: Url = callback
            .authorization_url(&profile, crate::auth::CLIENT_ID)
            .unwrap();
        browser.open(&url, &operation()).await.unwrap();
        assert_eq!(
            callback.receive(&profile, &operation()).await.unwrap(),
            "secret-code"
        );
    }

    #[test]
    fn published_metadata_matches_the_login_contract() {
        let metadata: serde_json::Value =
            serde_json::from_str(include_str!("../../site/oauth/client.json")).unwrap();
        assert_eq!(metadata["client_id"], crate::auth::CLIENT_ID);
        assert_eq!(
            metadata["redirect_uris"],
            serde_json::json!([crate::auth::REDIRECT_URI])
        );
        assert_eq!(metadata["token_endpoint_auth_method"], "none");
    }
}
