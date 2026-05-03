use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::HashMap;

const DEFAULT_BASE_URL: &str = "https://api.cloudflare.com/client/v4";

#[derive(Clone, Debug)]
pub struct Email {
    api_key: String,
    account_id: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl Email {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            account_id: std::env::var("CLOUDFLARE_ACCOUNT_ID")
                .or_else(|_| std::env::var("CF_ACCOUNT_ID"))
                .ok(),
            base_url: DEFAULT_BASE_URL.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, Error> {
        let api_key = std::env::var("CLOUDFLARE_API_TOKEN")
            .or_else(|_| std::env::var("CF_API_TOKEN"))
            .map_err(|_| Error::MissingApiToken)?;

        Ok(Self::new(api_key))
    }

    pub fn with_account_id(mut self, account_id: impl Into<String>) -> Self {
        self.account_id = Some(account_id.into());
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    pub fn emails(&self) -> Emails<'_> {
        Emails { email: self }
    }

    async fn account_id(&self) -> Result<String, Error> {
        if let Some(account_id) = &self.account_id {
            return Ok(account_id.clone());
        }

        let accounts: Vec<Account> = self
            .request(reqwest::Method::GET, "/accounts", None)
            .await?;

        match accounts.as_slice() {
            [] => Err(Error::NoAccounts),
            [account] => Ok(account.id.clone()),
            _ => Err(Error::MultipleAccounts),
        }
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, Error> {
        if self.api_key.is_empty() {
            return Err(Error::MissingApiToken);
        }

        let url = format!("{}{}", self.base_url, path);
        let mut request = self
            .client
            .request(method, url)
            .header(AUTHORIZATION, format!("Bearer {}", self.api_key));

        if let Some(body) = body {
            request = request.header(CONTENT_TYPE, "application/json").json(&body);
        }

        let response = request.send().await?;
        let status = response.status();
        let payload = response.json::<CloudflareResponse<T>>().await?;

        if !status.is_success() || !payload.success {
            let api_error = payload.errors.first();

            return Err(Error::Api {
                status: status.as_u16(),
                code: api_error.map(ApiError::code),
                message: api_error
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| status.to_string()),
                details: Value::Array(payload.errors.into_iter().map(Value::from).collect()),
            });
        }

        payload.result.ok_or(Error::MissingResult)
    }
}

pub struct Emails<'a> {
    email: &'a Email,
}

impl Emails<'_> {
    pub async fn send(&self, message: SendEmail) -> Result<SendEmailResponse, Error> {
        self.send_with_options(message, SendOptions::default())
            .await
    }

    pub async fn create(&self, message: SendEmail) -> Result<SendEmailResponse, Error> {
        self.send(message).await
    }

    pub async fn send_with_options(
        &self,
        message: SendEmail,
        options: SendOptions,
    ) -> Result<SendEmailResponse, Error> {
        message.validate()?;

        let account_id = self.email.account_id().await?;
        let path = format!("/accounts/{account_id}/email/sending/send");
        let mut request = self
            .email
            .client
            .post(format!("{}{}", self.email.base_url, path))
            .header(AUTHORIZATION, format!("Bearer {}", self.email.api_key))
            .header(CONTENT_TYPE, "application/json");

        if let Some(idempotency_key) = options.idempotency_key {
            request = request.header("Idempotency-Key", idempotency_key);
        }

        let response = request.json(&message).send().await?;
        let status = response.status();
        let payload = response
            .json::<CloudflareResponse<CloudflareSendResult>>()
            .await?;

        if !status.is_success() || !payload.success {
            let api_error = payload.errors.first();

            return Err(Error::Api {
                status: status.as_u16(),
                code: api_error.map(ApiError::code),
                message: api_error
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| status.to_string()),
                details: Value::Array(payload.errors.into_iter().map(Value::from).collect()),
            });
        }

        let result = payload.result.ok_or(Error::MissingResult)?;

        Ok(SendEmailResponse {
            id: result.id(),
            delivered: result.delivered,
            queued: result.queued,
            permanent_bounces: result.permanent_bounces,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct SendOptions {
    pub idempotency_key: Option<String>,
}

impl SendOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SendEmail {
    pub from: Address,
    pub to: Recipients,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<Recipients>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Recipients>,
    #[serde(rename = "reply_to", skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Attachment>>,
}

impl SendEmail {
    pub fn new(
        from: impl Into<Address>,
        to: impl Into<Recipients>,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            subject: subject.into(),
            html: None,
            text: None,
            cc: None,
            bcc: None,
            reply_to: None,
            headers: None,
            attachments: None,
        }
    }

    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.html = Some(html.into());
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn cc(mut self, cc: impl Into<Recipients>) -> Self {
        self.cc = Some(cc.into());
        self
    }

    pub fn bcc(mut self, bcc: impl Into<Recipients>) -> Self {
        self.bcc = Some(bcc.into());
        self
    }

    pub fn reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers
            .get_or_insert_with(HashMap::new)
            .insert(name.into(), value.into());
        self
    }

    pub fn attachment(mut self, attachment: Attachment) -> Self {
        self.attachments
            .get_or_insert_with(Vec::new)
            .push(attachment);
        self
    }

    fn validate(&self) -> Result<(), Error> {
        if self.subject.is_empty() {
            return Err(Error::Validation("subject is required".to_string()));
        }

        if self.html.is_none() && self.text.is_none() {
            return Err(Error::Validation(
                "Either html or text is required".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Address {
    Email(String),
    Named { address: String, name: String },
}

impl Address {
    pub fn email(email: impl Into<String>) -> Self {
        Self::Email(email.into())
    }

    pub fn named(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self::Named {
            name: name.into(),
            address: email.into(),
        }
    }
}

impl From<&str> for Address {
    fn from(value: &str) -> Self {
        Self::Email(value.to_string())
    }
}

impl From<String> for Address {
    fn from(value: String) -> Self {
        Self::Email(value)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Recipients {
    One(String),
    Many(Vec<String>),
}

impl From<&str> for Recipients {
    fn from(value: &str) -> Self {
        Self::One(value.to_string())
    }
}

impl From<String> for Recipients {
    fn from(value: String) -> Self {
        Self::One(value)
    }
}

impl From<Vec<String>> for Recipients {
    fn from(value: Vec<String>) -> Self {
        Self::Many(value)
    }
}

impl<const N: usize> From<[&str; N]> for Recipients {
    fn from(value: [&str; N]) -> Self {
        Self::Many(value.into_iter().map(str::to_string).collect())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Attachment {
    pub content: String,
    pub filename: String,
    #[serde(rename = "type")]
    pub mime_type: String,
    pub disposition: AttachmentDisposition,
    #[serde(rename = "content_id", skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
}

impl Attachment {
    pub fn new(
        content: impl Into<String>,
        filename: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        Self {
            content: content.into(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            disposition: AttachmentDisposition::Attachment,
            content_id: None,
        }
    }

    pub fn inline(mut self, content_id: impl Into<String>) -> Self {
        self.disposition = AttachmentDisposition::Inline;
        self.content_id = Some(content_id.into());
        self
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentDisposition {
    Attachment,
    Inline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendEmailResponse {
    pub id: String,
    pub delivered: Vec<String>,
    pub queued: Vec<String>,
    pub permanent_bounces: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing Cloudflare API token")]
    MissingApiToken,
    #[error("No Cloudflare accounts found for this API token")]
    NoAccounts,
    #[error("Multiple Cloudflare accounts found; pass account_id explicitly")]
    MultipleAccounts,
    #[error("Cloudflare API error ({status}): {message}")]
    Api {
        status: u16,
        code: Option<String>,
        message: String,
        details: Value,
    },
    #[error("Cloudflare API response did not include a result")]
    MissingResult,
    #[error("Invalid email request: {0}")]
    Validation(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Deserialize)]
struct Account {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CloudflareResponse<T> {
    success: bool,
    errors: Vec<ApiError>,
    #[allow(dead_code)]
    messages: Vec<Value>,
    result: Option<T>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ApiError {
    code: Value,
    message: String,
}

impl ApiError {
    fn code(&self) -> String {
        match &self.code {
            Value::String(code) => code.clone(),
            code => code.to_string(),
        }
    }
}

impl From<ApiError> for Value {
    fn from(error: ApiError) -> Self {
        serde_json::json!({
            "code": error.code,
            "message": error.message,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CloudflareSendResult {
    #[serde(default, alias = "messageId", alias = "message_id")]
    id: Option<String>,
    #[serde(default)]
    delivered: Vec<String>,
    #[serde(default)]
    queued: Vec<String>,
    #[serde(default)]
    permanent_bounces: Vec<String>,
}

impl CloudflareSendResult {
    fn id(&self) -> String {
        self.id.clone().unwrap_or_else(|| {
            self.delivered
                .iter()
                .chain(self.queued.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        })
    }
}
