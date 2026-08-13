//! Amazon SP-API Reports v2021-06-30 worker and deterministic analysis.
//!
//! The production transport intentionally uses Login with Amazon OAuth only.
//! It does not implement legacy AWS IAM/SigV4 signing. Credentials are resolved
//! from environment-backed secret references and are never serialised or logged.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use flate2::read::GzDecoder;
use reqwest::StatusCode;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};

use db::marketplace::{
    self, sha256, ClaimedAnalysisJob, ClaimedReportRun, MetricSnapshot, NormalizedMetric,
    ParsedMetric, ParsedSnapshot,
};
use db::modules;

const PROMPT_VERSION: &str = "marketplace-deterministic-v1";
const MAX_DOCUMENT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct AmazonReportRequest {
    pub seller_id: String,
    pub region: String,
    pub secret_ref: String,
    pub report_type: String,
    pub marketplace_id: String,
    pub data_start_time: Option<DateTime<Utc>>,
    pub data_end_time: Option<DateTime<Utc>>,
    pub report_options: Value,
}

#[derive(Debug, Clone)]
pub enum AmazonReportStatus {
    InProgress,
    Done { document_id: String },
    Cancelled,
    Fatal { message: String },
}

#[derive(Debug, Clone)]
pub struct AmazonReportDocument {
    pub document_id: String,
    pub url: String,
    pub compression_algorithm: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AmazonClientError {
    RateLimited { retry_after_seconds: Option<i64> },
    ExpiredDownloadUrl,
    Retryable(String),
    Permanent(String),
}

impl std::fmt::Display for AmazonClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited { .. } => formatter.write_str("Amazon rate limit reached"),
            Self::ExpiredDownloadUrl => formatter.write_str("Amazon pre-signed URL expired"),
            Self::Retryable(message) | Self::Permanent(message) => formatter.write_str(message),
        }
    }
}

#[async_trait]
pub trait AmazonReportsClient: Send + Sync {
    async fn create_report(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<String, AmazonClientError>;
    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        report_id: &str,
    ) -> Result<AmazonReportStatus, AmazonClientError>;
    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonReportDocument, AmazonClientError>;
    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<Vec<u8>, AmazonClientError>;
}

#[async_trait]
pub trait InsightProvider: Send + Sync {
    fn model_name(&self) -> &str;
    async fn analyse(&self, payload: &Value) -> Result<Value, String>;
}

#[derive(Clone)]
pub struct OpenAiCompatibleProvider {
    http: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAiCompatibleProvider {
    pub fn from_environment() -> Result<Option<Self>, reqwest::Error> {
        let Ok(endpoint) = std::env::var("AI_PROVIDER_ENDPOINT") else {
            return Ok(None);
        };
        let Ok(model) = std::env::var("AI_PROVIDER_MODEL") else {
            return Ok(None);
        };
        if endpoint.trim().is_empty() || model.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            endpoint,
            model,
            api_key: std::env::var("AI_PROVIDER_API_KEY")
                .ok()
                .filter(|key| !key.is_empty()),
        }))
    }
}

#[async_trait]
impl InsightProvider for OpenAiCompatibleProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn analyse(&self, payload: &Value) -> Result<Value, String> {
        let mut request = self.http.post(&self.endpoint).json(&json!({
            "model": self.model,
            "temperature": 0,
            "response_format": { "type": "json_object" },
            "messages": [
                {
                    "role": "system",
                    "content": "You analyse only aggregated marketplace metrics. Return JSON with facts, changes_since_last_run, overall_trend, anomalies, hypotheses, options (2-5), uncertainty, and missing_data. Never infer personal data or instruct automatic marketplace changes."
                },
                { "role": "user", "content": payload.to_string() }
            ]
        }));
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("AI provider request failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("AI provider returned HTTP {}", response.status()));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "AI provider returned invalid JSON envelope".to_owned())?;
        let content = body
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| "AI provider response lacks choices[0].message.content".to_owned())?;
        let candidate: Value = serde_json::from_str(content)
            .map_err(|_| "AI provider content is not valid JSON".to_owned())?;
        validate_provider_result(&candidate)?;
        Ok(candidate)
    }
}

fn validate_provider_result(candidate: &Value) -> Result<(), String> {
    let object = candidate
        .as_object()
        .ok_or_else(|| "AI provider result must be a JSON object".to_owned())?;
    for field in [
        "facts",
        "changes_since_last_run",
        "overall_trend",
        "anomalies",
        "hypotheses",
        "options",
        "uncertainty",
        "missing_data",
    ] {
        if !object.contains_key(field) {
            return Err(format!("AI provider result lacks {field}"));
        }
    }
    let options = object
        .get("options")
        .and_then(Value::as_array)
        .ok_or_else(|| "AI provider options must be an array".to_owned())?;
    if !(2..=5).contains(&options.len()) {
        return Err("AI provider must return two to five options".to_owned());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct LiveSecret {
    refresh_token: String,
    client_id: String,
    client_secret: String,
}

#[derive(Clone)]
pub struct LiveAmazonClient {
    http: reqwest::Client,
}

impl LiveAmazonClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map(|http| Self { http })
    }

    fn endpoint(region: &str) -> Result<&'static str, AmazonClientError> {
        match region {
            "na" => Ok("https://sellingpartnerapi-na.amazon.com"),
            "eu" => Ok("https://sellingpartnerapi-eu.amazon.com"),
            "fe" => Ok("https://sellingpartnerapi-fe.amazon.com"),
            _ => Err(AmazonClientError::Permanent(
                "unsupported Amazon region".to_owned(),
            )),
        }
    }

    fn secret_environment_key(secret_ref: &str) -> String {
        let normalized = secret_ref
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>();
        format!("AMAZON_SECRET_{normalized}")
    }

    fn load_secret(secret_ref: &str) -> Result<LiveSecret, AmazonClientError> {
        let key = Self::secret_environment_key(secret_ref);
        let raw = std::env::var(&key).map_err(|_| {
            AmazonClientError::Permanent(format!(
                "Amazon secret reference {secret_ref} is not configured"
            ))
        })?;
        serde_json::from_str(&raw).map_err(|_| {
            AmazonClientError::Permanent("Amazon secret has invalid JSON shape".to_owned())
        })
    }

    async fn access_token(&self, secret_ref: &str) -> Result<String, AmazonClientError> {
        let secret = Self::load_secret(secret_ref)?;
        let response = self
            .http
            .post("https://api.amazon.com/auth/o2/token")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", secret.refresh_token.as_str()),
                ("client_id", secret.client_id.as_str()),
                ("client_secret", secret.client_secret.as_str()),
            ])
            .send()
            .await
            .map_err(|error| {
                AmazonClientError::Retryable(format!("LWA OAuth request failed: {error}"))
            })?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(AmazonClientError::RateLimited {
                retry_after_seconds: retry_after(&response),
            });
        }
        if !response.status().is_success() {
            return Err(AmazonClientError::Permanent(format!(
                "LWA OAuth returned HTTP {}",
                response.status()
            )));
        }
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
        }
        response
            .json::<TokenResponse>()
            .await
            .map(|body| body.access_token)
            .map_err(|_| AmazonClientError::Permanent("LWA OAuth returned invalid JSON".to_owned()))
    }

    async fn authenticated(
        &self,
        request: &AmazonReportRequest,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, AmazonClientError> {
        let base = Self::endpoint(&request.region)?;
        let access_token = self.access_token(&request.secret_ref).await?;
        Ok(self
            .http
            .request(method, format!("{base}{path}"))
            .header("x-amz-access-token", access_token))
    }
}

fn retry_after(response: &reqwest::Response) -> Option<i64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
}

fn response_error(response: reqwest::Response, description: &str) -> AmazonClientError {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return AmazonClientError::RateLimited {
            retry_after_seconds: retry_after(&response),
        };
    }
    if response.status().is_server_error() {
        return AmazonClientError::Retryable(format!("{description}: HTTP {}", response.status()));
    }
    AmazonClientError::Permanent(format!("{description}: HTTP {}", response.status()))
}

#[async_trait]
impl AmazonReportsClient for LiveAmazonClient {
    async fn create_report(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<String, AmazonClientError> {
        let body = json!({
            "reportType": request.report_type,
            "marketplaceIds": [request.marketplace_id],
            "dataStartTime": request.data_start_time,
            "dataEndTime": request.data_end_time,
            "reportOptions": request.report_options,
        });
        let response = self
            .authenticated(
                request,
                reqwest::Method::POST,
                "/reports/2021-06-30/reports",
            )
            .await?
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                AmazonClientError::Retryable(format!("createReport failed: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(response_error(response, "createReport"));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CreateResponse {
            report_id: String,
        }
        response
            .json::<CreateResponse>()
            .await
            .map(|body| body.report_id)
            .map_err(|_| {
                AmazonClientError::Permanent("createReport returned invalid JSON".to_owned())
            })
    }

    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        report_id: &str,
    ) -> Result<AmazonReportStatus, AmazonClientError> {
        let response = self
            .authenticated(
                request,
                reqwest::Method::GET,
                &format!("/reports/2021-06-30/reports/{report_id}"),
            )
            .await?
            .send()
            .await
            .map_err(|error| AmazonClientError::Retryable(format!("getReport failed: {error}")))?;
        if !response.status().is_success() {
            return Err(response_error(response, "getReport"));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct GetResponse {
            processing_status: String,
            report_document_id: Option<String>,
        }
        let body = response.json::<GetResponse>().await.map_err(|_| {
            AmazonClientError::Permanent("getReport returned invalid JSON".to_owned())
        })?;
        match body.processing_status.as_str() {
            "DONE" => body
                .report_document_id
                .map(|document_id| AmazonReportStatus::Done { document_id })
                .ok_or_else(|| {
                    AmazonClientError::Permanent("DONE report lacks reportDocumentId".to_owned())
                }),
            "CANCELLED" => Ok(AmazonReportStatus::Cancelled),
            "FATAL" => Ok(AmazonReportStatus::Fatal {
                message: "Amazon marked report processing as FATAL".to_owned(),
            }),
            _ => Ok(AmazonReportStatus::InProgress),
        }
    }

    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonReportDocument, AmazonClientError> {
        let response = self
            .authenticated(
                request,
                reqwest::Method::GET,
                &format!("/reports/2021-06-30/documents/{document_id}"),
            )
            .await?
            .send()
            .await
            .map_err(|error| {
                AmazonClientError::Retryable(format!("getReportDocument failed: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(response_error(response, "getReportDocument"));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DocumentResponse {
            url: String,
            compression_algorithm: Option<String>,
        }
        response
            .json::<DocumentResponse>()
            .await
            .map(|body| AmazonReportDocument {
                document_id: document_id.to_owned(),
                url: body.url,
                compression_algorithm: body.compression_algorithm,
            })
            .map_err(|_| {
                AmazonClientError::Permanent("getReportDocument returned invalid JSON".to_owned())
            })
    }

    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<Vec<u8>, AmazonClientError> {
        let response = self.http.get(&document.url).send().await.map_err(|error| {
            AmazonClientError::Retryable(format!("report document download failed: {error}"))
        })?;
        if response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::NOT_FOUND
        {
            return Err(AmazonClientError::ExpiredDownloadUrl);
        }
        if !response.status().is_success() {
            return Err(response_error(response, "report document download"));
        }
        let content = response.bytes().await.map_err(|error| {
            AmazonClientError::Retryable(format!("report document body failed: {error}"))
        })?;
        if content.len() > MAX_DOCUMENT_BYTES {
            return Err(AmazonClientError::Permanent(
                "report document exceeds size limit".to_owned(),
            ));
        }
        Ok(content.to_vec())
    }
}

#[derive(Clone, Default)]
pub struct FixtureAmazonClient;

const SALES_FIXTURE: &str = r#"{
  "salesAndTrafficByDate": [
    {"date":"2026-08-01","salesByAsin":[
      {"parentAsin":"B0PARENT01","childAsin":"B0DEMO001","salesByAsin":{"orderedProductSales":{"amount":"120.50","currencyCode":"EUR"},"unitsOrdered":10},"trafficByAsin":{"sessions":100,"pageViews":150,"unitSessionPercentage":10.0}},
      {"parentAsin":"B0PARENT02","childAsin":"B0DEMO002","salesByAsin":{"orderedProductSales":{"amount":"35.00","currencyCode":"EUR"},"unitsOrdered":2},"trafficByAsin":{"sessions":50,"pageViews":60,"unitSessionPercentage":4.0}}
    ]},
    {"date":"2026-08-02","salesByAsin":[
      {"parentAsin":"B0PARENT01","childAsin":"B0DEMO001","salesByAsin":{"orderedProductSales":{"amount":"98.00","currencyCode":"EUR"},"unitsOrdered":8},"trafficByAsin":{"sessions":95,"pageViews":145,"unitSessionPercentage":8.42}}
    ]}
  ]
}"#;

const INVENTORY_FIXTURE: &str = "snapshot-date\tsku\tasin\tavailable\tunits-shipped-t30\tcurrency\talert\textra-column\n2026-08-02\tSKU-DEMO-001\tB0DEMO001\t12\t18\tEUR\tlow-inventory\taccepted\n2026-08-02\tSKU-DEMO-002\tB0DEMO002\t2\t3\tEUR\texcess\taccepted\n";

#[async_trait]
impl AmazonReportsClient for FixtureAmazonClient {
    async fn create_report(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<String, AmazonClientError> {
        if request.secret_ref.contains("rate-limit") {
            return Err(AmazonClientError::RateLimited {
                retry_after_seconds: Some(1),
            });
        }
        Ok(format!("fixture:{}", request.report_type))
    }

    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        _report_id: &str,
    ) -> Result<AmazonReportStatus, AmazonClientError> {
        if request.secret_ref.contains("cancelled") {
            return Ok(AmazonReportStatus::Cancelled);
        }
        if request.secret_ref.contains("fatal") {
            return Ok(AmazonReportStatus::Fatal {
                message: "Synthetic fatal status".to_owned(),
            });
        }
        if request.secret_ref.contains("pending") {
            return Ok(AmazonReportStatus::InProgress);
        }
        Ok(AmazonReportStatus::Done {
            document_id: format!("fixture-document:{}", request.report_type),
        })
    }

    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonReportDocument, AmazonClientError> {
        Ok(AmazonReportDocument {
            document_id: document_id.to_owned(),
            url: format!("fixture://{}/{}", request.secret_ref, request.report_type),
            compression_algorithm: if request.secret_ref.contains("gzip") {
                Some("GZIP".to_owned())
            } else {
                None
            },
        })
    }

    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<Vec<u8>, AmazonClientError> {
        if document.url.contains("expired") {
            return Err(AmazonClientError::ExpiredDownloadUrl);
        }
        let content = if document.url.contains("broken") {
            b"not a valid report".to_vec()
        } else if document.url.contains(marketplace::SALES_AND_TRAFFIC) {
            SALES_FIXTURE.as_bytes().to_vec()
        } else if document.url.contains(marketplace::INVENTORY_PLANNING) {
            INVENTORY_FIXTURE.as_bytes().to_vec()
        } else {
            b"unparsed synthetic report".to_vec()
        };
        if document.compression_algorithm.as_deref() == Some("GZIP") {
            use flate2::{write::GzEncoder, Compression};
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            std::io::Write::write_all(&mut encoder, &content)
                .map_err(|error| AmazonClientError::Permanent(error.to_string()))?;
            encoder
                .finish()
                .map_err(|error| AmazonClientError::Permanent(error.to_string()))
        } else {
            Ok(content)
        }
    }
}

#[derive(Clone)]
pub struct CompositeAmazonClient {
    live: LiveAmazonClient,
    fixture: FixtureAmazonClient,
}

impl CompositeAmazonClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            live: LiveAmazonClient::new()?,
            fixture: FixtureAmazonClient,
        })
    }

    fn client(&self, request: &AmazonReportRequest) -> &dyn AmazonReportsClient {
        if request.secret_ref.starts_with("fixture:") {
            &self.fixture
        } else {
            &self.live
        }
    }
}

#[async_trait]
impl AmazonReportsClient for CompositeAmazonClient {
    async fn create_report(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<String, AmazonClientError> {
        self.client(request).create_report(request).await
    }

    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        report_id: &str,
    ) -> Result<AmazonReportStatus, AmazonClientError> {
        self.client(request).get_report(request, report_id).await
    }

    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonReportDocument, AmazonClientError> {
        self.client(request)
            .get_report_document(request, document_id)
            .await
    }

    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<Vec<u8>, AmazonClientError> {
        if document.url.starts_with("fixture://") {
            self.fixture.download_document(document).await
        } else {
            self.live.download_document(document).await
        }
    }
}

#[derive(Clone)]
pub struct MarketplaceWorker {
    client: Arc<dyn AmazonReportsClient>,
    insight_provider: Option<Arc<dyn InsightProvider>>,
}

impl MarketplaceWorker {
    pub fn new(
        client: Arc<dyn AmazonReportsClient>,
        insight_provider: Option<Arc<dyn InsightProvider>>,
    ) -> Self {
        Self {
            client,
            insight_provider,
        }
    }

    pub async fn cycle(&self, pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        if !modules::is_enabled(pool, modules::MARKETPLACE_INTELLIGENCE).await? {
            return Ok(());
        }
        marketplace::enqueue_due_schedules(pool, 25).await?;
        for run in marketplace::claim_due_runs(pool, 10).await? {
            self.process_run(pool, run).await;
        }
        for job in marketplace::claim_analysis_jobs(pool, 10).await? {
            self.process_analysis(pool, job).await;
        }
        Ok(())
    }

    async fn process_run(&self, pool: &sqlx::PgPool, run: ClaimedReportRun) {
        let request = AmazonReportRequest {
            seller_id: run.seller_id.clone(),
            region: run.region.clone(),
            secret_ref: run.secret_ref.clone(),
            report_type: run.report_type.clone(),
            marketplace_id: run.marketplace_id.clone(),
            data_start_time: run.data_start_time,
            data_end_time: run.data_end_time,
            report_options: run.report_options.clone(),
        };
        if request.seller_id.trim().is_empty() {
            let _ = marketplace::mark_run_terminal(
                pool,
                run.id,
                "failed",
                "seller_context_missing",
                "Amazon seller context is required",
            )
            .await;
            return;
        }
        // The API cannot fetch a report for a missing declared role. Persist
        // this as an explicit error rather than attempting unauthorised calls.
        let connection = marketplace::AmazonConnection {
            id: run.connection_id,
            seller_id: run.seller_id.clone(),
            region: run.region.clone(),
            secret_ref: run.secret_ref.clone(),
            granted_roles: run.granted_roles.clone(),
            mode: run.mode.clone(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        if !marketplace::report_type_is_allowed_for_connection(&connection, &run.report_type) {
            let _ = marketplace::mark_run_terminal(
                pool,
                run.id,
                "failed",
                "required_role_missing",
                "The configured seller connection does not grant the required report role",
            )
            .await;
            return;
        }
        if !marketplace::report_options_are_supported(&run.report_type, &run.report_options) {
            let _ = marketplace::mark_run_terminal(
                pool,
                run.id,
                "failed",
                "unsupported_options",
                "The requested options are not allowlisted for this report type",
            )
            .await;
            return;
        }

        match run.status.as_str() {
            "queued" | "requesting" => {
                let _ = marketplace::mark_run_requesting(pool, run.id).await;
                match self.client.create_report(&request).await {
                    Ok(report_id) => {
                        let _ =
                            marketplace::set_run_request_created(pool, run.id, &report_id).await;
                    }
                    Err(error) => {
                        self.record_client_error(pool, run.id, "requesting", error)
                            .await
                    }
                }
            }
            "polling" => {
                let Some(report_id) = run.amazon_report_id.as_deref() else {
                    let _ = marketplace::mark_run_terminal(
                        pool,
                        run.id,
                        "failed",
                        "missing_report_id",
                        "Polling state lacks an Amazon report ID",
                    )
                    .await;
                    return;
                };
                match self.client.get_report(&request, report_id).await {
                    Ok(AmazonReportStatus::InProgress) => {
                        let delay = exponential_backoff(run.poll_attempts, 60, 3600);
                        let _ = marketplace::set_run_poll_pending(
                            pool,
                            run.id,
                            delay,
                            "Amazon report is still processing",
                        )
                        .await;
                    }
                    Ok(AmazonReportStatus::Done { document_id }) => {
                        let _ =
                            marketplace::set_run_document_ready(pool, run.id, &document_id).await;
                    }
                    Ok(AmazonReportStatus::Cancelled) => {
                        let _ = marketplace::mark_run_terminal(
                            pool,
                            run.id,
                            "cancelled",
                            "cancelled",
                            "Amazon cancelled the report or returned no data",
                        )
                        .await;
                    }
                    Ok(AmazonReportStatus::Fatal { message }) => {
                        let _ = marketplace::mark_run_terminal(
                            pool, run.id, "fatal", "fatal", &message,
                        )
                        .await;
                    }
                    Err(error) => {
                        self.record_client_error(pool, run.id, "polling", error)
                            .await
                    }
                }
            }
            "downloading" => {
                let Some(document_id) = run.amazon_report_document_id.as_deref() else {
                    let _ = marketplace::mark_run_terminal(
                        pool,
                        run.id,
                        "failed",
                        "missing_document_id",
                        "Download state lacks an Amazon report document ID",
                    )
                    .await;
                    return;
                };
                match self.client.get_report_document(&request, document_id).await {
                    Ok(document) => match self.client.download_document(&document).await {
                        Ok(downloaded) => {
                            match decompress(&downloaded, document.compression_algorithm.as_deref())
                            {
                                Ok(content) => {
                                    let _ = marketplace::archive_document(
                                        pool,
                                        run.id,
                                        &document.document_id,
                                        Some(content_type_for(&run.report_type)),
                                        document.compression_algorithm.as_deref(),
                                        &content,
                                    )
                                    .await;
                                }
                                Err(message) => {
                                    let _ = marketplace::mark_run_failure(
                                        pool,
                                        run.id,
                                        "decompression_failed",
                                        &message,
                                        None,
                                    )
                                    .await;
                                }
                            }
                        }
                        Err(AmazonClientError::ExpiredDownloadUrl) => {
                            let _ = marketplace::retry_run(
                                pool,
                                run.id,
                                "downloading",
                                "download_url_expired",
                                "Amazon download URL expired; requesting a fresh document URL",
                                exponential_backoff(run.attempts, 5, 300),
                            )
                            .await;
                        }
                        Err(error) => {
                            self.record_client_error(pool, run.id, "downloading", error)
                                .await
                        }
                    },
                    Err(error) => {
                        self.record_client_error(pool, run.id, "downloading", error)
                            .await
                    }
                }
            }
            "parsing" => match marketplace::load_document_for_parsing(pool, run.id).await {
                Ok(Some((_document_id, content))) => match parse_report(&run, &content) {
                    Ok(Some(parsed)) => {
                        if let Err(error) = marketplace::store_snapshot(pool, &run, &parsed).await {
                            let _ = marketplace::retry_run(
                                pool,
                                run.id,
                                "parsing",
                                "snapshot_store_failed",
                                &format!("Could not persist normalized snapshot: {error}"),
                                exponential_backoff(run.attempts, 5, 300),
                            )
                            .await;
                        }
                    }
                    Ok(None) => {
                        let _ = marketplace::mark_run_archived(
                            pool,
                            run.id,
                            None,
                            "No parser is registered for this report type; raw document retained",
                        )
                        .await;
                    }
                    Err(error) => {
                        let version = marketplace::report_definition(&run.report_type)
                            .and_then(|definition| definition.parser_version)
                            .unwrap_or("unknown-parser");
                        let _ =
                            marketplace::mark_parse_failure(pool, run.id, version, &error).await;
                    }
                },
                Ok(None) => {
                    let _ = marketplace::mark_run_terminal(
                        pool,
                        run.id,
                        "failed",
                        "raw_document_missing",
                        "Cannot parse because the archived raw document is missing",
                    )
                    .await;
                }
                Err(error) => {
                    let _ = marketplace::retry_run(
                        pool,
                        run.id,
                        "parsing",
                        "database_error",
                        &format!("Could not read raw report archive: {error}"),
                        exponential_backoff(run.attempts, 5, 300),
                    )
                    .await;
                }
            },
            _ => {}
        }
    }

    async fn record_client_error(
        &self,
        pool: &sqlx::PgPool,
        run_id: uuid::Uuid,
        state: &str,
        error: AmazonClientError,
    ) {
        match error {
            AmazonClientError::RateLimited {
                retry_after_seconds,
            } => {
                let delay = retry_after_seconds.unwrap_or(60).max(1);
                let _ = marketplace::retry_run(
                    pool,
                    run_id,
                    state,
                    "rate_limited",
                    "Amazon returned HTTP 429; retry scheduled with backoff",
                    delay,
                )
                .await;
            }
            AmazonClientError::Retryable(message) => {
                let _ =
                    marketplace::retry_run(pool, run_id, state, "temporary_error", &message, 60)
                        .await;
            }
            AmazonClientError::ExpiredDownloadUrl => {
                let _ = marketplace::retry_run(
                    pool,
                    run_id,
                    "downloading",
                    "download_url_expired",
                    "Amazon download URL expired; requesting a fresh document URL",
                    5,
                )
                .await;
            }
            AmazonClientError::Permanent(message) => {
                let _ = marketplace::mark_run_terminal(
                    pool,
                    run_id,
                    "failed",
                    "amazon_error",
                    &message,
                )
                .await;
            }
        }
    }

    async fn process_analysis(&self, pool: &sqlx::PgPool, job: ClaimedAnalysisJob) {
        let result = match job.analysis_type.as_str() {
            "delta" => deterministic_delta(pool, &job).await,
            "total" => deterministic_total(pool, &job).await,
            _ => Err("Unknown analysis type".to_owned()),
        };
        match result {
            Ok(mut result) => {
                let payload = allowlisted_ai_payload(&result);
                let mut strategy = "deterministic";
                let mut model_name = None;
                if let Some(provider) = &self.insight_provider {
                    match provider.analyse(&payload).await {
                        Ok(provider_result) => {
                            result["provider_insight"] = provider_result;
                            strategy = "deterministic_with_provider";
                            model_name = Some(provider.model_name());
                        }
                        Err(error) => {
                            result["provider_status"] =
                                json!({ "status": "failed", "message": error });
                        }
                    }
                } else {
                    result["provider_status"] = json!({ "status": "disabled" });
                }
                let _ = marketplace::complete_analysis(
                    pool,
                    &job,
                    strategy,
                    model_name,
                    PROMPT_VERSION,
                    &sha256(payload.to_string().as_bytes()),
                    &result,
                )
                .await;
            }
            Err(error) => {
                let _ = marketplace::fail_analysis(pool, job.id, &error).await;
            }
        }
    }
}

fn exponential_backoff(attempt: i32, base_seconds: i64, max_seconds: i64) -> i64 {
    let exponent = u32::try_from(attempt.clamp(0, 10)).unwrap_or(10);
    base_seconds
        .saturating_mul(2_i64.pow(exponent))
        .min(max_seconds)
}

fn content_type_for(report_type: &str) -> &'static str {
    marketplace::report_definition(report_type)
        .map(|definition| match definition.format {
            "json" => "application/json",
            _ => "text/tab-separated-values",
        })
        .unwrap_or("application/octet-stream")
}

fn decompress(content: &[u8], compression: Option<&str>) -> Result<Vec<u8>, String> {
    match compression {
        None | Some("NONE") => Ok(content.to_vec()),
        Some("GZIP") => {
            let mut decoder = GzDecoder::new(content);
            let mut output = Vec::new();
            decoder
                .read_to_end(&mut output)
                .map_err(|error| format!("Could not decompress GZIP report: {error}"))?;
            if output.len() > MAX_DOCUMENT_BYTES {
                return Err("Decompressed report exceeds size limit".to_owned());
            }
            Ok(output)
        }
        Some(algorithm) => Err(format!(
            "Unsupported Amazon compression algorithm {algorithm}"
        )),
    }
}

fn parse_report(run: &ClaimedReportRun, raw: &[u8]) -> Result<Option<ParsedSnapshot>, String> {
    match run.report_type.as_str() {
        marketplace::SALES_AND_TRAFFIC => {
            parse_sales_and_traffic(raw, run.data_start_time, run.data_end_time).map(Some)
        }
        marketplace::INVENTORY_PLANNING => parse_inventory_planning(raw).map(Some),
        _ => Ok(None),
    }
}

fn decimal(value: Option<&Value>, name: &str) -> Result<Decimal, String> {
    let Some(value) = value else {
        return Err(format!("Missing required field {name}"));
    };
    match value {
        Value::String(value) => {
            Decimal::from_str_exact(value).map_err(|_| format!("Field {name} is not a decimal"))
        }
        Value::Number(value) => Decimal::from_str_exact(&value.to_string())
            .map_err(|_| format!("Field {name} is not a decimal")),
        _ => Err(format!("Field {name} is not a decimal")),
    }
}

fn optional_decimal(value: Option<&Value>) -> Option<Decimal> {
    value.and_then(|value| match value {
        Value::String(value) => Decimal::from_str_exact(value).ok(),
        Value::Number(value) => Decimal::from_str_exact(&value.to_string()).ok(),
        _ => None,
    })
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, part| current.get(*part))
}

fn as_rows(value: &Value) -> Vec<(&Value, Option<NaiveDate>)> {
    let mut rows = Vec::new();
    if let Some(days) = value.get("salesAndTrafficByDate").and_then(Value::as_array) {
        for day in days {
            let date = day
                .get("date")
                .and_then(Value::as_str)
                .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok());
            if let Some(asin_rows) = day
                .get("salesByAsin")
                .or_else(|| day.get("salesAndTrafficByAsin"))
                .and_then(Value::as_array)
            {
                for row in asin_rows {
                    rows.push((row, date));
                }
            }
        }
    }
    if rows.is_empty() {
        if let Some(asin_rows) = value
            .get("salesAndTrafficByAsin")
            .or_else(|| value.get("salesByAsin"))
            .and_then(Value::as_array)
        {
            rows.extend(asin_rows.iter().map(|row| (row, None)));
        }
    }
    rows
}

fn parse_sales_and_traffic(
    raw: &[u8],
    requested_start: Option<DateTime<Utc>>,
    requested_end: Option<DateTime<Utc>>,
) -> Result<ParsedSnapshot, String> {
    let value: Value =
        serde_json::from_slice(raw).map_err(|error| format!("Invalid sales JSON: {error}"))?;
    let rows = as_rows(&value);
    if rows.is_empty() {
        return Err(
            "Sales & Traffic report lacks salesAndTrafficByDate or salesAndTrafficByAsin rows"
                .to_owned(),
        );
    }
    let mut metrics = Vec::new();
    let mut total_sales = Decimal::ZERO;
    let mut total_units = Decimal::ZERO;
    let mut total_sessions = Decimal::ZERO;
    let mut total_page_views = Decimal::ZERO;
    let mut currencies = BTreeMap::<String, Decimal>::new();
    let mut dates = Vec::new();
    for (row, row_date) in rows {
        let asin = row
            .get("childAsin")
            .or_else(|| row.get("asin"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Sales & Traffic row lacks childAsin".to_owned())?;
        if let Some(date) = row_date {
            dates.push(date);
        }
        let sales = decimal(
            value_at(row, &["salesByAsin", "orderedProductSales", "amount"]),
            "orderedProductSales.amount",
        )?;
        let currency = value_at(row, &["salesByAsin", "orderedProductSales", "currencyCode"])
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWN")
            .to_owned();
        let units = decimal(
            value_at(row, &["salesByAsin", "unitsOrdered"]),
            "unitsOrdered",
        )?;
        let sessions = optional_decimal(value_at(row, &["trafficByAsin", "sessions"]))
            .unwrap_or(Decimal::ZERO);
        let page_views = optional_decimal(value_at(row, &["trafficByAsin", "pageViews"]))
            .unwrap_or(Decimal::ZERO);
        total_sales += sales;
        total_units += units;
        total_sessions += sessions;
        total_page_views += page_views;
        *currencies.entry(currency.clone()).or_default() += sales;
        let evidence = json!({ "asin": asin, "date": row_date.map(|value| value.to_string()) });
        let asin_dimension_key = row_date
            .map(|date| format!("{asin}:{date}"))
            .unwrap_or_else(|| asin.to_owned());
        metrics.extend([
            ParsedMetric {
                metric_name: "ordered_product_sales".to_owned(),
                dimension_type: "asin_date".to_owned(),
                dimension_key: asin_dimension_key.clone(),
                value_numeric: sales,
                unit: "currency".to_owned(),
                currency_code: Some(currency.clone()),
                evidence: evidence.clone(),
            },
            ParsedMetric {
                metric_name: "units_ordered".to_owned(),
                dimension_type: "asin_date".to_owned(),
                dimension_key: asin_dimension_key.clone(),
                value_numeric: units,
                unit: "units".to_owned(),
                currency_code: None,
                evidence: evidence.clone(),
            },
            ParsedMetric {
                metric_name: "sessions".to_owned(),
                dimension_type: "asin_date".to_owned(),
                dimension_key: asin_dimension_key.clone(),
                value_numeric: sessions,
                unit: "sessions".to_owned(),
                currency_code: None,
                evidence: evidence.clone(),
            },
            ParsedMetric {
                metric_name: "page_views".to_owned(),
                dimension_type: "asin_date".to_owned(),
                dimension_key: asin_dimension_key,
                value_numeric: page_views,
                unit: "views".to_owned(),
                currency_code: None,
                evidence,
            },
        ]);
    }
    if currencies.len() > 1 {
        return Err(
            "Sales & Traffic report contains multiple currencies; no silent aggregation".to_owned(),
        );
    }
    let currency = currencies.keys().next().cloned();
    let start = dates
        .iter()
        .min()
        .copied()
        .map(|date| Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap()))
        .or(requested_start);
    let end = dates
        .iter()
        .max()
        .copied()
        .map(|date| Utc.from_utc_datetime(&date.and_hms_opt(23, 59, 59).unwrap()))
        .or(requested_end);
    let period_days = match (start, end) {
        (Some(start), Some(end)) => (end.date_naive() - start.date_naive()).num_days() + 1,
        _ => 0,
    };
    let conversion = if total_sessions.is_zero() {
        Decimal::ZERO
    } else {
        (total_units / total_sessions * Decimal::from(100)).round_dp(4)
    };
    metrics.extend([
        ParsedMetric {
            metric_name: "ordered_product_sales".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_sales,
            unit: "currency".to_owned(),
            currency_code: currency.clone(),
            evidence: json!({ "aggregation": "sum_by_asin" }),
        },
        ParsedMetric {
            metric_name: "units_ordered".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_units,
            unit: "units".to_owned(),
            currency_code: None,
            evidence: json!({ "aggregation": "sum_by_asin" }),
        },
        ParsedMetric {
            metric_name: "sessions".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_sessions,
            unit: "sessions".to_owned(),
            currency_code: None,
            evidence: json!({ "aggregation": "sum_by_asin" }),
        },
        ParsedMetric {
            metric_name: "conversion_rate".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: conversion,
            unit: "percent".to_owned(),
            currency_code: None,
            evidence: json!({ "formula": "units_ordered / sessions * 100" }),
        },
    ]);
    Ok(ParsedSnapshot {
        parser_version: "sales-traffic-json-v1".to_owned(),
        period_start: start,
        period_end: end,
        granularity: "daily_asin".to_owned(),
        comparability_key: format!("sales-traffic:daily_asin:{period_days}d"),
        summary: json!({
            "ordered_product_sales": total_sales.to_string(),
            "currency": currency,
            "units_ordered": total_units.to_string(),
            "sessions": total_sessions.to_string(),
            "page_views": total_page_views.to_string(),
            "conversion_rate": conversion.to_string(),
            "period_days": period_days,
        }),
        metrics,
    })
}

fn parse_inventory_planning(raw: &[u8]) -> Result<ParsedSnapshot, String> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| "Inventory Planning report is not UTF-8".to_owned())?;
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Err("Inventory Planning report is empty".to_owned());
    };
    let names = header
        .split('\t')
        .map(|name| name.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let index = |name: &str| names.iter().position(|candidate| candidate == name);
    let sku_index = index("sku")
        .ok_or_else(|| "Inventory Planning report lacks required sku column".to_owned())?;
    let available_index = index("available")
        .ok_or_else(|| "Inventory Planning report lacks required available column".to_owned())?;
    let asin_index = index("asin");
    let date_index = index("snapshot-date");
    let shipped_index = index("units-shipped-t30");
    let alert_index = index("alert");
    let mut metrics = Vec::new();
    let mut total_available = Decimal::ZERO;
    let mut total_shipped = Decimal::ZERO;
    let mut snapshot_dates = Vec::new();
    let mut rows = 0;
    for (line_number, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let values = line.split('\t').collect::<Vec<_>>();
        let get = |column: usize| values.get(column).map(|value| value.trim()).unwrap_or("");
        let sku = get(sku_index);
        if sku.is_empty() {
            return Err(format!(
                "Inventory Planning row {} lacks sku",
                line_number + 2
            ));
        }
        let available = Decimal::from_str_exact(get(available_index)).map_err(|_| {
            format!(
                "Inventory Planning row {} has invalid available",
                line_number + 2
            )
        })?;
        let asin = asin_index
            .map(get)
            .filter(|value| !value.is_empty())
            .unwrap_or(sku);
        let row_date = date_index
            .map(get)
            .filter(|value| !value.is_empty())
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d"))
            .transpose()
            .map_err(|_| {
                format!(
                    "Inventory Planning row {} has invalid snapshot-date",
                    line_number + 2
                )
            })?;
        if let Some(date) = row_date {
            snapshot_dates.push(date);
        }
        let evidence = json!({
            "sku": sku,
            "asin": asin,
            "snapshot_date": row_date.map(|value| value.to_string()),
            "alert": alert_index.map(get).filter(|value| !value.is_empty()),
        });
        total_available += available;
        metrics.push(ParsedMetric {
            metric_name: "available_inventory".to_owned(),
            dimension_type: "sku".to_owned(),
            dimension_key: sku.to_owned(),
            value_numeric: available,
            unit: "units".to_owned(),
            currency_code: None,
            evidence: evidence.clone(),
        });
        if let Some(shipped_index) = shipped_index {
            let shipped = Decimal::from_str_exact(get(shipped_index)).map_err(|_| {
                format!(
                    "Inventory Planning row {} has invalid units-shipped-t30",
                    line_number + 2
                )
            })?;
            total_shipped += shipped;
            metrics.push(ParsedMetric {
                metric_name: "units_shipped_t30".to_owned(),
                dimension_type: "sku".to_owned(),
                dimension_key: sku.to_owned(),
                value_numeric: shipped,
                unit: "units_30d".to_owned(),
                currency_code: None,
                evidence,
            });
        }
        rows += 1;
    }
    if rows == 0 {
        return Err("Inventory Planning report has no data rows".to_owned());
    }
    let stock_cover_days = if total_shipped.is_zero() {
        None
    } else {
        Some((total_available / total_shipped * Decimal::from(30)).round_dp(2))
    };
    metrics.extend([
        ParsedMetric {
            metric_name: "available_inventory".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_available,
            unit: "units".to_owned(),
            currency_code: None,
            evidence: json!({ "aggregation": "sum_by_sku" }),
        },
        ParsedMetric {
            metric_name: "units_shipped_t30".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_shipped,
            unit: "units_30d".to_owned(),
            currency_code: None,
            evidence: json!({ "aggregation": "sum_by_sku" }),
        },
    ]);
    if let Some(stock_cover_days) = stock_cover_days {
        metrics.push(ParsedMetric {
            metric_name: "stock_cover_days".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: stock_cover_days,
            unit: "days".to_owned(),
            currency_code: None,
            evidence: json!({ "formula": "available_inventory / units_shipped_t30 * 30" }),
        });
    }
    let date = snapshot_dates.iter().max().copied();
    Ok(ParsedSnapshot {
        parser_version: "inventory-planning-tsv-v1".to_owned(),
        period_start: date.map(|date| Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())),
        period_end: date.map(|date| Utc.from_utc_datetime(&date.and_hms_opt(23, 59, 59).unwrap())),
        granularity: "current_sku".to_owned(),
        comparability_key: "inventory-planning:current_sku".to_owned(),
        summary: json!({
            "available_inventory": total_available.to_string(),
            "units_shipped_t30": total_shipped.to_string(),
            "stock_cover_days": stock_cover_days.map(|value| value.to_string()),
            "row_count": rows,
        }),
        metrics,
    })
}

fn metric_map(
    metrics: Vec<NormalizedMetric>,
) -> HashMap<(String, String, String, String, Option<String>), NormalizedMetric> {
    metrics
        .into_iter()
        .map(|metric| {
            (
                (
                    metric.metric_name.clone(),
                    metric.dimension_type.clone(),
                    metric.dimension_key.clone(),
                    metric.unit.clone(),
                    metric.currency_code.clone(),
                ),
                metric,
            )
        })
        .collect()
}

fn evidence_ref(snapshot: &MetricSnapshot, metric: &NormalizedMetric) -> String {
    format!("snapshot:{}:metric:{}", snapshot.id, metric.id)
}

async fn deterministic_delta(
    pool: &sqlx::PgPool,
    job: &ClaimedAnalysisJob,
) -> Result<Value, String> {
    let run_id = job
        .run_id
        .ok_or_else(|| "Delta analysis requires a report run".to_owned())?;
    let current = marketplace::snapshot_for_run(pool, run_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Delta analysis lacks a normalized snapshot".to_owned())?;
    let current_metrics = marketplace::metrics_for_snapshot(pool, current.id)
        .await
        .map_err(|error| error.to_string())?;
    let facts = current_metrics
        .iter()
        .filter(|metric| metric.dimension_type == "catalog")
        .map(|metric| {
            json!({
                "metric": metric.metric_name,
                "value": metric.value_numeric.to_string(),
                "unit": metric.unit,
                "currency": metric.currency_code,
                "evidence_ref": evidence_ref(&current, metric),
            })
        })
        .collect::<Vec<_>>();
    let Some(previous) = marketplace::previous_compatible_snapshot(pool, &current)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(json!({
            "facts": facts,
            "changes_since_last_run": [],
            "overall_trend": "No comparable earlier snapshot is available.",
            "anomalies": [],
            "hypotheses": [],
            "options": generic_options(),
            "uncertainty": "high",
            "missing_data": ["A previous successful snapshot with matching report type, granularity and period length is required for a delta analysis."],
            "recommendation_notice": "Recommendations only; Merchant does not make Amazon changes.",
        }));
    };
    let previous_metrics = metric_map(
        marketplace::metrics_for_snapshot(pool, previous.id)
            .await
            .map_err(|error| error.to_string())?,
    );
    let mut changes = Vec::new();
    let mut anomalies = Vec::new();
    for metric in current_metrics
        .iter()
        .filter(|metric| metric.dimension_type == "catalog")
    {
        let key = (
            metric.metric_name.clone(),
            metric.dimension_type.clone(),
            metric.dimension_key.clone(),
            metric.unit.clone(),
            metric.currency_code.clone(),
        );
        let Some(previous_metric) = previous_metrics.get(&key) else {
            continue;
        };
        let difference = metric.value_numeric - previous_metric.value_numeric;
        let percentage = if previous_metric.value_numeric.is_zero() {
            None
        } else {
            Some((difference / previous_metric.value_numeric * Decimal::from(100)).round_dp(2))
        };
        let change = json!({
            "metric": metric.metric_name,
            "current": metric.value_numeric.to_string(),
            "previous": previous_metric.value_numeric.to_string(),
            "difference": difference.to_string(),
            "percent_change": percentage.map(|value| value.to_string()),
            "unit": metric.unit,
            "evidence_refs": [evidence_ref(&current, metric), evidence_ref(&previous, previous_metric)],
        });
        if percentage.is_some_and(|value| value.abs() >= Decimal::from(20)) {
            anomalies.push(json!({
                "kind": "material_change",
                "metric": metric.metric_name,
                "detail": change,
            }));
        }
        changes.push(change);
    }
    let options = options_for_metrics(&current_metrics);
    let trend = if changes.is_empty() {
        "No matching catalog metrics were available for comparison.".to_owned()
    } else {
        "Comparable metrics were calculated against the immediately preceding compatible snapshot."
            .to_owned()
    };
    Ok(json!({
        "facts": facts,
        "changes_since_last_run": changes,
        "overall_trend": trend,
        "anomalies": anomalies,
        "hypotheses": hypotheses_for_metrics(&current_metrics),
        "options": options,
        "uncertainty": if anomalies.is_empty() { "medium" } else { "medium; material changes need operational validation" },
        "missing_data": missing_data_for_metrics(&current_metrics),
        "recommendation_notice": "Recommendations only; Merchant does not make Amazon changes.",
    }))
}

async fn deterministic_total(
    pool: &sqlx::PgPool,
    job: &ClaimedAnalysisJob,
) -> Result<Value, String> {
    let snapshots = marketplace::snapshots_for_window(pool, job)
        .await
        .map_err(|error| error.to_string())?;
    if snapshots.is_empty() {
        return Ok(json!({
            "facts": [], "changes_since_last_run": [], "overall_trend": "No snapshots in selected period.",
            "anomalies": [], "hypotheses": [], "options": generic_options(), "uncertainty": "high",
            "missing_data": ["No normalized snapshots exist in the selected period."],
            "recommendation_notice": "Recommendations only; Merchant does not make Amazon changes.",
        }));
    }
    let keys = snapshots
        .iter()
        .map(|snapshot| snapshot.comparability_key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if keys.len() != 1 {
        return Ok(json!({
            "facts": [], "changes_since_last_run": [], "overall_trend": "Analysis was not aggregated.",
            "anomalies": [], "hypotheses": [], "options": generic_options(), "uncertainty": "high",
            "missing_data": ["Selected snapshots have incompatible granularity or reporting periods and are intentionally not compared."],
            "recommendation_notice": "Recommendations only; Merchant does not make Amazon changes.",
        }));
    }
    let mut series = Vec::new();
    for snapshot in &snapshots {
        let metrics = marketplace::metrics_for_snapshot(pool, snapshot.id)
            .await
            .map_err(|error| error.to_string())?;
        series.push(json!({
            "snapshot_id": snapshot.id,
            "period_start": snapshot.period_start,
            "period_end": snapshot.period_end,
            "catalog_metrics": metrics.iter().filter(|metric| metric.dimension_type == "catalog").map(|metric| json!({
                "metric": metric.metric_name, "value": metric.value_numeric.to_string(), "unit": metric.unit,
                "currency": metric.currency_code, "evidence_ref": evidence_ref(snapshot, metric),
            })).collect::<Vec<_>>(),
        }));
    }
    let seasonality = if snapshots.len() >= 12 {
        "Sufficient snapshot count for a tentative seasonality review; no causal claim is made."
    } else {
        "Seasonality is not assessed: fewer than twelve comparable snapshots are available."
    };
    Ok(json!({
        "facts": series,
        "changes_since_last_run": [],
        "overall_trend": format!("{} comparable snapshots were retained in chronological order.", snapshots.len()),
        "seasonality": seasonality,
        "anomalies": [],
        "hypotheses": [],
        "options": generic_options(),
        "uncertainty": "medium",
        "missing_data": ["Trend interpretation is deterministic and does not infer causality."],
        "recommendation_notice": "Recommendations only; Merchant does not make Amazon changes.",
    }))
}

fn generic_options() -> Vec<Value> {
    vec![
        json!({
            "action": "Review data coverage before changing operations",
            "expected_effect": "Reduces decisions based on incomplete reporting.",
            "effort": "low",
            "risks": ["Delays operational response while data is collected."],
            "evidence_refs": [],
            "uncertainty": "medium",
        }),
        json!({
            "action": "Run the same compatible report period again on schedule",
            "expected_effect": "Builds a comparable baseline for later delta analysis.",
            "effort": "low",
            "risks": ["Amazon report availability and processing time vary."],
            "evidence_refs": [],
            "uncertainty": "low",
        }),
    ]
}

fn options_for_metrics(metrics: &[NormalizedMetric]) -> Vec<Value> {
    let mut options = generic_options();
    if let Some(cover) = metrics.iter().find(|metric| {
        metric.metric_name == "stock_cover_days" && metric.dimension_type == "catalog"
    }) {
        options.push(json!({
            "action": "Review replenishment timing for FBA inventory",
            "expected_effect": "May reduce stock-out risk if the reported coverage is low.",
            "effort": "medium",
            "risks": ["Over-ordering increases storage and cash commitment."],
            "evidence_refs": [format!("snapshot:{}:metric:{}", cover.snapshot_id, cover.id)],
            "uncertainty": "medium; inbound inventory and lead times are not in this slice",
        }));
    }
    if let Some(conversion) = metrics.iter().find(|metric| {
        metric.metric_name == "conversion_rate" && metric.dimension_type == "catalog"
    }) {
        options.push(json!({
            "action": "Review detail-page content and offer competitiveness",
            "expected_effect": "May improve conversion if traffic remains stable.",
            "effort": "medium",
            "risks": ["No causal attribution; changes must be separately measured."],
            "evidence_refs": [format!("snapshot:{}:metric:{}", conversion.snapshot_id, conversion.id)],
            "uncertainty": "high",
        }));
    }
    options.truncate(5);
    options
}

fn hypotheses_for_metrics(metrics: &[NormalizedMetric]) -> Vec<Value> {
    metrics
        .iter()
        .filter(|metric| {
            metric.metric_name == "stock_cover_days" && metric.value_numeric < Decimal::from(14)
        })
        .map(|metric| {
            json!({
                "hypothesis": "Low reported stock cover could constrain future sales.",
                "evidence_refs": [format!("snapshot:{}:metric:{}", metric.snapshot_id, metric.id)],
                "uncertainty": "medium; inbound and non-Amazon stock are not included.",
            })
        })
        .collect()
}

fn missing_data_for_metrics(metrics: &[NormalizedMetric]) -> Vec<&'static str> {
    let names = metrics
        .iter()
        .map(|metric| metric.metric_name.as_str())
        .collect::<Vec<_>>();
    let mut missing = Vec::new();
    if !names.contains(&"ordered_product_sales") {
        missing.push("Revenue and conversion are not present in this report type.");
    }
    if !names.contains(&"available_inventory") {
        missing.push("Inventory coverage is not present in this report type.");
    }
    missing
}

pub fn allowlisted_ai_payload(result: &Value) -> Value {
    let allowed = [
        "ordered_product_sales",
        "units_ordered",
        "sessions",
        "page_views",
        "conversion_rate",
        "available_inventory",
        "units_shipped_t30",
        "stock_cover_days",
    ];
    let facts = result
        .get("facts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|fact| {
            fact.get("metric")
                .and_then(Value::as_str)
                .is_some_and(|metric| allowed.contains(&metric))
        })
        .cloned()
        .collect::<Vec<_>>();
    json!({ "facts": facts, "changes_since_last_run": result.get("changes_since_last_run").cloned().unwrap_or_else(|| json!([])) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sales_json_without_binary_money() {
        let parsed = parse_sales_and_traffic(SALES_FIXTURE.as_bytes(), None, None).unwrap();
        assert_eq!(parsed.parser_version, "sales-traffic-json-v1");
        assert!(parsed
            .metrics
            .iter()
            .any(|metric| metric.metric_name == "conversion_rate"));
        assert_eq!(parsed.summary["ordered_product_sales"], "253.50");
    }

    #[test]
    fn inventory_parser_tolerates_unknown_columns() {
        let parsed = parse_inventory_planning(INVENTORY_FIXTURE.as_bytes()).unwrap();
        assert_eq!(parsed.summary["available_inventory"], "14");
        assert!(parsed
            .metrics
            .iter()
            .any(|metric| metric.metric_name == "stock_cover_days"));
    }

    #[test]
    fn missing_inventory_required_field_is_visible() {
        assert!(parse_inventory_planning(b"sku\nSKU-1\n").is_err());
    }

    #[test]
    fn compressed_document_is_decoded() {
        use flate2::{write::GzEncoder, Compression};
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut encoder, b"fixture").unwrap();
        let compressed = encoder.finish().unwrap();
        assert_eq!(decompress(&compressed, Some("GZIP")).unwrap(), b"fixture");
    }

    #[test]
    fn ai_payload_removes_unallowlisted_fields() {
        let payload = allowlisted_ai_payload(&json!({
            "facts": [
                { "metric": "sessions", "value": "10" },
                { "metric": "buyer_email", "value": "private@example.test" }
            ]
        }));
        assert_eq!(payload["facts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn invalid_provider_result_is_rejected() {
        assert!(validate_provider_result(&json!({ "options": [] })).is_err());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn fixture_worker_is_restart_safe_and_creates_one_snapshot(pool: sqlx::PgPool) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let connection = db::marketplace::create_demo_connection(&pool)
            .await
            .unwrap();
        let input = db::marketplace::CreateAmazonReportRunInput {
            marketplace_id: "A1PA6795UKMFR9".to_owned(),
            report_type: marketplace::SALES_AND_TRAFFIC.to_owned(),
            data_start_time: Some(Utc::now() - chrono::Duration::days(7)),
            data_end_time: Some(Utc::now()),
            report_options: json!({}),
        };
        let first = db::marketplace::create_manual_run(&pool, connection.id, &input)
            .await
            .unwrap();
        let duplicate = db::marketplace::create_manual_run(&pool, connection.id, &input)
            .await
            .unwrap();
        assert_eq!(first.id, duplicate.id);

        for _ in 0..5 {
            MarketplaceWorker::new(Arc::new(FixtureAmazonClient), None)
                .cycle(&pool)
                .await
                .unwrap();
        }
        let detail = db::marketplace::get_run_detail(&pool, first.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.run.status, "succeeded");
        assert!(detail.document.is_some());
        assert!(detail.snapshot.is_some());
        assert_eq!(
            detail.analyses[0].result["provider_status"]["status"],
            "disabled"
        );
        let snapshots: i64 =
            sqlx::query_scalar("SELECT count(*) FROM amazon_metric_snapshots WHERE run_id = $1")
                .bind(first.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(snapshots, 1);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn fixture_terminal_and_retry_states_preserve_raw_data(pool: sqlx::PgPool) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient), None);
        for (suffix, expected_status) in [("cancelled", "cancelled"), ("fatal", "fatal")] {
            let connection = db::marketplace::upsert_connection(
                &pool,
                &db::marketplace::AmazonConnectionInput {
                    seller_id: format!("DEMO-{suffix}"),
                    region: "eu".to_owned(),
                    secret_ref: format!("fixture:{suffix}"),
                    granted_roles: vec!["Brand Analytics".to_owned()],
                    marketplace_ids: vec!["A1PA6795UKMFR9".to_owned()],
                    mode: "fixture".to_owned(),
                    enabled: true,
                },
            )
            .await
            .unwrap();
            let run = db::marketplace::create_manual_run(
                &pool,
                connection.id,
                &db::marketplace::CreateAmazonReportRunInput {
                    marketplace_id: "A1PA6795UKMFR9".to_owned(),
                    report_type: marketplace::SALES_AND_TRAFFIC.to_owned(),
                    data_start_time: None,
                    data_end_time: None,
                    report_options: json!({}),
                },
            )
            .await
            .unwrap();
            worker.cycle(&pool).await.unwrap();
            worker.cycle(&pool).await.unwrap();
            let detail = db::marketplace::get_run_detail(&pool, run.id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(detail.run.status, expected_status);
            assert!(detail.document.is_none());
        }

        let connection = db::marketplace::upsert_connection(
            &pool,
            &db::marketplace::AmazonConnectionInput {
                seller_id: "DEMO-RATE".to_owned(),
                region: "eu".to_owned(),
                secret_ref: "fixture:rate-limit".to_owned(),
                granted_roles: vec!["Brand Analytics".to_owned()],
                marketplace_ids: vec!["A1PA6795UKMFR9".to_owned()],
                mode: "fixture".to_owned(),
                enabled: true,
            },
        )
        .await
        .unwrap();
        let run = db::marketplace::create_manual_run(
            &pool,
            connection.id,
            &db::marketplace::CreateAmazonReportRunInput {
                marketplace_id: "A1PA6795UKMFR9".to_owned(),
                report_type: marketplace::SALES_AND_TRAFFIC.to_owned(),
                data_start_time: None,
                data_end_time: None,
                report_options: json!({}),
            },
        )
        .await
        .unwrap();
        worker.cycle(&pool).await.unwrap();
        let detail = db::marketplace::get_run_detail(&pool, run.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.run.failure_code.as_deref(), Some("rate_limited"));
        assert_eq!(detail.run.status, "requesting");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn enabled_schedule_uses_the_same_fixture_job_path(pool: sqlx::PgPool) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let connection = db::marketplace::create_demo_connection(&pool)
            .await
            .unwrap();
        db::marketplace::upsert_schedule(
            &pool,
            connection.id,
            &db::marketplace::AmazonReportScheduleInput {
                marketplace_id: "A1PA6795UKMFR9".to_owned(),
                report_type: marketplace::SALES_AND_TRAFFIC.to_owned(),
                report_options: json!({}),
                interval_seconds: 900,
                enabled: true,
            },
        )
        .await
        .unwrap();
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient), None);
        for _ in 0..5 {
            worker.cycle(&pool).await.unwrap();
        }
        let run: (String, String) = sqlx::query_as(
            "SELECT trigger_source, status FROM amazon_report_runs ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(run.0, "scheduled");
        assert_eq!(run.1, "succeeded");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn expired_urls_and_parser_errors_are_recoverable_without_losing_archives(
        pool: sqlx::PgPool,
    ) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient), None);
        for (suffix, expected_status, archive_expected) in [
            ("expired", "downloading", false),
            ("broken", "failed", true),
        ] {
            let connection = db::marketplace::upsert_connection(
                &pool,
                &db::marketplace::AmazonConnectionInput {
                    seller_id: format!("DEMO-{suffix}"),
                    region: "eu".to_owned(),
                    secret_ref: format!("fixture:{suffix}"),
                    granted_roles: vec!["Brand Analytics".to_owned()],
                    marketplace_ids: vec!["A1PA6795UKMFR9".to_owned()],
                    mode: "fixture".to_owned(),
                    enabled: true,
                },
            )
            .await
            .unwrap();
            let run = db::marketplace::create_manual_run(
                &pool,
                connection.id,
                &db::marketplace::CreateAmazonReportRunInput {
                    marketplace_id: "A1PA6795UKMFR9".to_owned(),
                    report_type: marketplace::SALES_AND_TRAFFIC.to_owned(),
                    data_start_time: None,
                    data_end_time: None,
                    report_options: json!({}),
                },
            )
            .await
            .unwrap();
            for _ in 0..4 {
                worker.cycle(&pool).await.unwrap();
            }
            let detail = db::marketplace::get_run_detail(&pool, run.id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(detail.run.status, expected_status);
            assert_eq!(detail.document.is_some(), archive_expected);
            if suffix == "expired" {
                assert_eq!(
                    detail.run.failure_code.as_deref(),
                    Some("download_url_expired")
                );
            } else {
                assert_eq!(detail.document.unwrap().import_status, "failed");
            }
        }
    }
}
