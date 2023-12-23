use eyre::Result;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::time::Duration;
use std::time::SystemTime;
use tracing::{debug, trace};

pub struct Client {
    token: String,
    base_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub pagination_token: Option<String>,
    pub result: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchMetadata {
    pub account_id: String,
    pub account_name: String,
    pub account_email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInstance {
    pub id: String,
    pub name: String,
    pub description: String,
    pub application_id: String,
    pub application_name: String,
    pub icon: String,
    pub search_metadata: Option<SearchMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub protocol: String,
    pub relay_state: Option<String>,
}

impl Client {
    /// # Errors
    ///
    /// The function will error for network issues, or if the response is not parseable as expected
    ///
    pub async fn new(org_id: &str, auth_code: &str, region: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct SsoTokenResponse {
            token: String,
        }

        // Set the region for the SSO client
        let base_url = format!("https://portal.sso.{region}.amazonaws.com");

        // Get SSO Token
        let response = reqwest::Client::new()
            .post(format!("{base_url}/auth/sso-token"))
            .form(&[("authCode", auth_code), ("orgId", org_id)])
            .send()
            .await?;

        let text = response.text().await?;
        trace!("Received {}", &text);

        let SsoTokenResponse { token } = serde_json::from_str(&text)?;

        Ok(Self { token, base_url })
    }

    /// # Errors
    ///
    /// The function will error for network issues, or if the response is not parseable as expected
    ///
    pub async fn app_instances(&self) -> Result<Vec<AppInstance>> {
        let response = reqwest::Client::new()
            .get(format!("{0}/instance/appinstances", &self.base_url))
            .header("x-amz-sso_bearer_token", &self.token)
            .header("x-amz-sso-bearer-token", &self.token)
            .send()
            .await?;

        let text = response.text().await?;
        trace!("Received {}", &text);

        let Page::<AppInstance> { result, .. } = serde_json::from_str(&text)?;
        Ok(result)
    }

    /// # Errors
    ///
    /// The function will error for network issues, or if the response is not parseable as expected
    ///
    pub async fn profiles(&self, app_instance_id: &str) -> Result<Vec<Profile>> {
        let response = reqwest::Client::new()
            .get(format!(
                "{0}/instance/appinstance/{app_instance_id}/profiles",
                self.base_url
            ))
            .header("x-amz-sso_bearer_token", &self.token)
            .header("x-amz-sso-bearer-token", &self.token)
            .send()
            .await?;

        let text = response.text().await?;
        trace!("Received {}", &text);

        let Page::<Profile> { result, .. } = serde_json::from_str(&text)?;
        Ok(result)
    }

    /// # Errors
    ///
    /// The function will error for network issues, or if the response is not parseable as expected
    ///
    pub async fn credentials(
        &self,
        account_id: &str,
        role_name: &str,
    ) -> Result<aws_credential_types::Credentials> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Credentials {
            access_key_id: String,
            secret_access_key: String,
            session_token: String,
            expiration: u64,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RoleCredentials {
            role_credentials: Credentials,
        }

        debug!("Requesting credentials for account: {account_id}, role: {role_name}");

        let RoleCredentials { role_credentials } = reqwest::Client::new()
            .get(format!("{0}/federation/credentials/", self.base_url))
            .query(&[
                ("account_id", account_id),
                ("role_name", role_name),
                ("debug", "true"),
            ])
            .header("x-amz-sso_bearer_token", &self.token)
            .header("x-amz-sso-bearer-token", &self.token)
            .send()
            .await?
            .json()
            .await?;

        Ok(aws_credential_types::Credentials::new(
            role_credentials.access_key_id,
            role_credentials.secret_access_key,
            Some(role_credentials.session_token),
            Some(SystemTime::UNIX_EPOCH + Duration::from_millis(role_credentials.expiration)),
            "oktaws",
        ))
    }
}

impl AppInstance {
    #[must_use]
    pub fn account_name(&self) -> Option<&str> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"\((.+)\)").unwrap();
        }

        RE.captures(&self.name)
            .and_then(|captures| captures.get(1))
            .map(|mat| mat.as_str())
    }

    #[must_use]
    pub fn account_id(&self) -> Option<&str> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^(\d+)").unwrap();
        }

        RE.captures(&self.name)
            .and_then(|captures| captures.get(1))
            .map(|mat| mat.as_str())
    }
}
