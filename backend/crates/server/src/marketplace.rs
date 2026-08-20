//! Amazon SP-API Reports v2021-06-30 worker and deterministic analysis.
//!
//! The production transport intentionally uses Login with Amazon OAuth only.
//! It does not implement legacy AWS IAM/SigV4 signing. Credentials are resolved
//! from environment-backed secret references and are never serialised or logged.

use std::collections::{BTreeMap, HashMap, HashSet};
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

const RULESET_VERSION: &str = "marketplace-rules-v2";
const MAX_DOCUMENT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmazonOperation {
    LwaTokenRefresh,
    CreateReport,
    GetReport,
    GetReportDocument,
    DownloadReportDocument,
}

impl AmazonOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LwaTokenRefresh => "lwa_token_refresh",
            Self::CreateReport => "create_report",
            Self::GetReport => "get_report",
            Self::GetReportDocument => "get_report_document",
            Self::DownloadReportDocument => "download_report_document",
        }
    }
}

pub const AMAZON_PILOT_OPERATION_ALLOWLIST: &[AmazonOperation] = &[
    AmazonOperation::LwaTokenRefresh,
    AmazonOperation::CreateReport,
    AmazonOperation::GetReport,
    AmazonOperation::GetReportDocument,
    AmazonOperation::DownloadReportDocument,
];

#[derive(Debug, Clone)]
pub struct AmazonTransportObservation {
    pub operation: AmazonOperation,
    pub request_id_redacted: Option<String>,
    pub rate_limit_limit: Option<String>,
    pub retry_after_seconds: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AmazonClientResponse<T> {
    pub value: T,
    pub observations: Vec<AmazonTransportObservation>,
}

#[derive(Debug, Clone)]
pub struct AmazonReportRequest {
    pub seller_id: String,
    pub region: String,
    pub secret_ref: String,
    pub mode: String,
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
    RateLimited {
        retry_after_seconds: Option<i64>,
        observation: AmazonTransportObservation,
    },
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
    ) -> Result<AmazonClientResponse<String>, AmazonClientError>;
    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        report_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportStatus>, AmazonClientError>;
    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportDocument>, AmazonClientError>;
    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<AmazonClientResponse<Vec<u8>>, AmazonClientError>;
}

#[derive(Debug, Clone, Deserialize)]
struct LiveSecret {
    refresh_token: String,
    client_id: String,
    client_secret: String,
}

#[derive(Debug, Deserialize)]
struct AmazonStagingApproval {
    seller_sha256: String,
    region: String,
    marketplace_id: String,
}

#[derive(Clone)]
pub struct LiveAmazonClient {
    http: reqwest::Client,
    endpoint_override: Option<String>,
    lwa_endpoint: String,
    secret_override: Option<LiveSecret>,
    provider_secrets: Option<crate::provider_secrets::ProviderSecretStore>,
}

impl LiveAmazonClient {
    pub fn new(
        provider_secrets: crate::provider_secrets::ProviderSecretStore,
    ) -> Result<Self, reqwest::Error> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map(|http| Self {
                http,
                endpoint_override: None,
                lwa_endpoint: "https://api.amazon.com/auth/o2/token".to_owned(),
                secret_override: None,
                provider_secrets: Some(provider_secrets),
            })
    }

    #[cfg(test)]
    fn for_fake_server(endpoint: String) -> Result<Self, reqwest::Error> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map(|http| Self {
                http,
                endpoint_override: Some(endpoint.clone()),
                lwa_endpoint: format!("{endpoint}/auth/o2/token"),
                secret_override: Some(LiveSecret {
                    refresh_token: "synthetic-refresh".to_owned(),
                    client_id: "synthetic-client".to_owned(),
                    client_secret: "synthetic-secret".to_owned(),
                }),
                provider_secrets: None,
            })
    }

    fn endpoint(&self, region: &str) -> Result<String, AmazonClientError> {
        if let Some(endpoint) = &self.endpoint_override {
            return Ok(endpoint.clone());
        }
        match region {
            "na" => Ok("https://sellingpartnerapi-na.amazon.com".to_owned()),
            "eu" => Ok("https://sellingpartnerapi-eu.amazon.com".to_owned()),
            "fe" => Ok("https://sellingpartnerapi-fe.amazon.com".to_owned()),
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

    async fn load_secret(&self, secret_ref: &str) -> Result<LiveSecret, AmazonClientError> {
        if let Some(secret) = &self.secret_override {
            return Ok(secret.clone());
        }
        if secret_ref == db::provider_secrets::PILOT_AMAZON_SECRET_REF {
            if let Some(store) = &self.provider_secrets {
                match store.amazon_credentials().await {
                    Ok(Some(secret)) => {
                        return Ok(LiveSecret {
                            refresh_token: secret.refresh_token,
                            client_id: secret.client_id,
                            client_secret: secret.client_secret,
                        });
                    }
                    Ok(None) => {}
                    Err(_) => {
                        return Err(AmazonClientError::Permanent(
                            "Amazon provider credential storage is unavailable".to_owned(),
                        ));
                    }
                }
            }
        }
        let key = Self::secret_environment_key(secret_ref);
        let raw = std::env::var(&key).map_err(|_| {
            AmazonClientError::Permanent(format!(
                "Amazon secret reference {secret_ref} is not configured"
            ))
        })?;
        let secret: LiveSecret = serde_json::from_str(&raw).map_err(|_| {
            AmazonClientError::Permanent("Amazon secret has invalid JSON shape".to_owned())
        })?;
        if secret.refresh_token.trim().is_empty()
            || secret.client_id.trim().is_empty()
            || secret.client_secret.trim().is_empty()
        {
            return Err(AmazonClientError::Permanent(
                "Amazon secret has invalid JSON shape".to_owned(),
            ));
        }
        Ok(secret)
    }

    async fn access_token(
        &self,
        secret_ref: &str,
    ) -> Result<AmazonClientResponse<String>, AmazonClientError> {
        let secret = self.load_secret(secret_ref).await?;
        let response = self
            .http
            .post(&self.lwa_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", secret.refresh_token.as_str()),
                ("client_id", secret.client_id.as_str()),
                ("client_secret", secret.client_secret.as_str()),
            ])
            .send()
            .await
            .map_err(|_| AmazonClientError::Retryable("LWA OAuth transport failed".to_owned()))?;
        let observation = transport_observation(&response, AmazonOperation::LwaTokenRefresh);
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(AmazonClientError::RateLimited {
                retry_after_seconds: retry_after(&response),
                observation,
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
            .map(|body| AmazonClientResponse {
                value: body.access_token,
                observations: vec![observation],
            })
            .map_err(|_| AmazonClientError::Permanent("LWA OAuth returned invalid JSON".to_owned()))
    }

    async fn authenticated(
        &self,
        request: &AmazonReportRequest,
        operation: AmazonOperation,
        resource_id: Option<&str>,
    ) -> Result<(reqwest::RequestBuilder, Vec<AmazonTransportObservation>), AmazonClientError> {
        if !AMAZON_PILOT_OPERATION_ALLOWLIST.contains(&operation) {
            return Err(AmazonClientError::Permanent(
                "operation is outside the Amazon pilot allowlist".to_owned(),
            ));
        }
        let base = self.endpoint(&request.region)?;
        let valid_resource_id = |value: &str| {
            !value.is_empty()
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        };
        let (method, path) = match (operation, resource_id) {
            (AmazonOperation::CreateReport, None) => (
                reqwest::Method::POST,
                "/reports/2021-06-30/reports".to_owned(),
            ),
            (AmazonOperation::GetReport, Some(report_id)) if valid_resource_id(report_id) => (
                reqwest::Method::GET,
                format!("/reports/2021-06-30/reports/{report_id}"),
            ),
            (AmazonOperation::GetReportDocument, Some(document_id))
                if valid_resource_id(document_id) =>
            {
                (
                    reqwest::Method::GET,
                    format!("/reports/2021-06-30/documents/{document_id}"),
                )
            }
            _ => {
                return Err(AmazonClientError::Permanent(
                    "operation is outside the Amazon pilot allowlist".to_owned(),
                ));
            }
        };
        let access_token = self.access_token(&request.secret_ref).await?;
        Ok((
            self.http
                .request(method, format!("{base}{path}"))
                .header("x-amz-access-token", access_token.value),
            access_token.observations,
        ))
    }
}

fn retry_after(response: &reqwest::Response) -> Option<i64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
}

fn transport_observation(
    response: &reqwest::Response,
    operation: AmazonOperation,
) -> AmazonTransportObservation {
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .filter(|value| {
                value.len() <= 256 && value.chars().all(|character| !character.is_control())
            })
    };
    let request_id_redacted = header("x-amzn-requestid").map(|request_id| {
        let digest = sha256(request_id.as_bytes());
        format!("sha256:{}", &digest[..12])
    });
    let rate_limit_limit =
        header("x-amzn-ratelimit-limit").map(|value| value.chars().take(64).collect::<String>());
    AmazonTransportObservation {
        operation,
        request_id_redacted,
        rate_limit_limit,
        retry_after_seconds: retry_after(response),
    }
}

fn response_error(
    response: reqwest::Response,
    description: &str,
    operation: AmazonOperation,
) -> AmazonClientError {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        let observation = transport_observation(&response, operation);
        return AmazonClientError::RateLimited {
            retry_after_seconds: retry_after(&response),
            observation,
        };
    }
    if response.status().is_server_error() {
        return AmazonClientError::Retryable(format!("{description}: HTTP {}", response.status()));
    }
    AmazonClientError::Permanent(format!("{description}: HTTP {}", response.status()))
}

fn download_url_allowed(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    #[cfg(test)]
    if url.scheme() == "http" && matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return true;
    }
    url.scheme() == "https"
        && (host == "amazonaws.com"
            || host.ends_with(".amazonaws.com")
            || host == "cloudfront.net"
            || host.ends_with(".cloudfront.net"))
}

#[async_trait]
impl AmazonReportsClient for LiveAmazonClient {
    async fn create_report(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<AmazonClientResponse<String>, AmazonClientError> {
        let body = json!({
            "reportType": request.report_type,
            "marketplaceIds": [request.marketplace_id],
            "dataStartTime": request.data_start_time,
            "dataEndTime": request.data_end_time,
            "reportOptions": request.report_options,
        });
        let (builder, mut observations) = self
            .authenticated(request, AmazonOperation::CreateReport, None)
            .await?;
        let response = builder.json(&body).send().await.map_err(|_| {
            AmazonClientError::Retryable("createReport transport failed".to_owned())
        })?;
        if !response.status().is_success() {
            return Err(response_error(
                response,
                "createReport",
                AmazonOperation::CreateReport,
            ));
        }
        observations.push(transport_observation(
            &response,
            AmazonOperation::CreateReport,
        ));
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CreateResponse {
            report_id: String,
        }
        response
            .json::<CreateResponse>()
            .await
            .map(|body| AmazonClientResponse {
                value: body.report_id,
                observations,
            })
            .map_err(|_| {
                AmazonClientError::Permanent("createReport returned invalid JSON".to_owned())
            })
    }

    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        report_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportStatus>, AmazonClientError> {
        let (builder, mut observations) = self
            .authenticated(request, AmazonOperation::GetReport, Some(report_id))
            .await?;
        let response = builder
            .send()
            .await
            .map_err(|_| AmazonClientError::Retryable("getReport transport failed".to_owned()))?;
        if !response.status().is_success() {
            return Err(response_error(
                response,
                "getReport",
                AmazonOperation::GetReport,
            ));
        }
        observations.push(transport_observation(&response, AmazonOperation::GetReport));
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct GetResponse {
            processing_status: String,
            report_document_id: Option<String>,
        }
        let body = response.json::<GetResponse>().await.map_err(|_| {
            AmazonClientError::Permanent("getReport returned invalid JSON".to_owned())
        })?;
        let value = match body.processing_status.as_str() {
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
        }?;
        Ok(AmazonClientResponse {
            value,
            observations,
        })
    }

    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportDocument>, AmazonClientError> {
        let (builder, mut observations) = self
            .authenticated(
                request,
                AmazonOperation::GetReportDocument,
                Some(document_id),
            )
            .await?;
        let response = builder.send().await.map_err(|_| {
            AmazonClientError::Retryable("getReportDocument transport failed".to_owned())
        })?;
        if !response.status().is_success() {
            return Err(response_error(
                response,
                "getReportDocument",
                AmazonOperation::GetReportDocument,
            ));
        }
        observations.push(transport_observation(
            &response,
            AmazonOperation::GetReportDocument,
        ));
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DocumentResponse {
            url: String,
            compression_algorithm: Option<String>,
        }
        response
            .json::<DocumentResponse>()
            .await
            .map(|body| AmazonClientResponse {
                value: AmazonReportDocument {
                    document_id: document_id.to_owned(),
                    url: body.url,
                    compression_algorithm: body.compression_algorithm,
                },
                observations,
            })
            .map_err(|_| {
                AmazonClientError::Permanent("getReportDocument returned invalid JSON".to_owned())
            })
    }

    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<AmazonClientResponse<Vec<u8>>, AmazonClientError> {
        if !download_url_allowed(&document.url) {
            return Err(AmazonClientError::Permanent(
                "report document URL is outside the approved Amazon download boundary".to_owned(),
            ));
        }
        let response = self.http.get(&document.url).send().await.map_err(|_| {
            AmazonClientError::Retryable("report document download transport failed".to_owned())
        })?;
        if response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::NOT_FOUND
        {
            return Err(AmazonClientError::ExpiredDownloadUrl);
        }
        if !response.status().is_success() {
            return Err(response_error(
                response,
                "report document download",
                AmazonOperation::DownloadReportDocument,
            ));
        }
        let observation = transport_observation(&response, AmazonOperation::DownloadReportDocument);
        let content = response.bytes().await.map_err(|_| {
            AmazonClientError::Retryable("report document body transport failed".to_owned())
        })?;
        if content.len() > MAX_DOCUMENT_BYTES {
            return Err(AmazonClientError::Permanent(
                "report document exceeds size limit".to_owned(),
            ));
        }
        Ok(AmazonClientResponse {
            value: content.to_vec(),
            observations: vec![observation],
        })
    }
}

#[derive(Clone, Default)]
pub struct FixtureAmazonClient;

fn fixture_observation(operation: AmazonOperation) -> AmazonTransportObservation {
    let digest = sha256(format!("synthetic:{}", operation.as_str()).as_bytes());
    AmazonTransportObservation {
        operation,
        request_id_redacted: Some(format!("sha256:{}", &digest[..12])),
        rate_limit_limit: Some("synthetic".to_owned()),
        retry_after_seconds: None,
    }
}

fn fixture_response<T>(operation: AmazonOperation, value: T) -> AmazonClientResponse<T> {
    AmazonClientResponse {
        value,
        observations: vec![fixture_observation(operation)],
    }
}

const SALES_FIXTURE: &str = r#"{
  "reportSpecification": {
    "reportType": "GET_SALES_AND_TRAFFIC_REPORT",
    "reportOptions": {"dateGranularity":"DAY","asinGranularity":"CHILD"},
    "dataStartTime": "2026-08-01",
    "dataEndTime": "2026-08-02",
    "marketplaceIds": ["A1PA6795UKMFR9"]
  },
  "salesAndTrafficByDate": [
    {"date":"2026-08-01","salesByDate":{"orderedProductSales":{"amount":"155.50","currencyCode":"EUR"},"unitsOrdered":12},"trafficByDate":{"sessions":150,"pageViews":210}},
    {"date":"2026-08-02","salesByDate":{"orderedProductSales":{"amount":"98.00","currencyCode":"EUR"},"unitsOrdered":8},"trafficByDate":{"sessions":95,"pageViews":145}}
  ],
  "salesAndTrafficByAsin": [
    {"parentAsin":"B0PARENT01","childAsin":"B0DEMO001","salesByAsin":{"orderedProductSales":{"amount":"218.50","currencyCode":"EUR"},"unitsOrdered":18},"trafficByAsin":{"sessions":195,"pageViews":295,"unitSessionPercentage":9.23},"futureField":"accepted"},
    {"parentAsin":"B0PARENT02","childAsin":"B0DEMO002","salesByAsin":{"orderedProductSales":{"amount":"35.00","currencyCode":"EUR"},"unitsOrdered":2},"trafficByAsin":{"sessions":50,"pageViews":60,"unitSessionPercentage":4.0}}
  ]
}"#;

const INVENTORY_FIXTURE: &str = "snapshot-date\tsku\tasin\tavailable\tunits-shipped-t30\tcurrency\talert\textra-column\n2026-08-02\tSKU-DEMO-001\tB0DEMO001\t12\t18\tEUR\tlow-inventory\taccepted\n2026-08-02\tSKU-DEMO-002\tB0DEMO002\t2\t3\tEUR\texcess\taccepted\n";

#[async_trait]
impl AmazonReportsClient for FixtureAmazonClient {
    async fn create_report(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<AmazonClientResponse<String>, AmazonClientError> {
        if request.secret_ref.contains("rate-limit") {
            let mut observation = fixture_observation(AmazonOperation::CreateReport);
            observation.retry_after_seconds = Some(1);
            return Err(AmazonClientError::RateLimited {
                retry_after_seconds: Some(1),
                observation,
            });
        }
        Ok(fixture_response(
            AmazonOperation::CreateReport,
            format!("fixture:{}", request.report_type),
        ))
    }

    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        _report_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportStatus>, AmazonClientError> {
        if request.secret_ref.contains("cancelled") {
            return Ok(fixture_response(
                AmazonOperation::GetReport,
                AmazonReportStatus::Cancelled,
            ));
        }
        if request.secret_ref.contains("fatal") {
            return Ok(fixture_response(
                AmazonOperation::GetReport,
                AmazonReportStatus::Fatal {
                    message: "Synthetic fatal status".to_owned(),
                },
            ));
        }
        if request.secret_ref.contains("pending") {
            return Ok(fixture_response(
                AmazonOperation::GetReport,
                AmazonReportStatus::InProgress,
            ));
        }
        Ok(fixture_response(
            AmazonOperation::GetReport,
            AmazonReportStatus::Done {
                document_id: format!("fixture-document:{}", request.report_type),
            },
        ))
    }

    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportDocument>, AmazonClientError> {
        let period_query = match (request.data_start_time, request.data_end_time) {
            (Some(start), Some(end)) => {
                format!("?start={}&end={}", start.date_naive(), end.date_naive())
            }
            _ => String::new(),
        };
        Ok(fixture_response(
            AmazonOperation::GetReportDocument,
            AmazonReportDocument {
                document_id: document_id.to_owned(),
                url: format!(
                    "fixture://{}/{}{}",
                    request.secret_ref, request.report_type, period_query
                ),
                compression_algorithm: if request.secret_ref.contains("gzip") {
                    Some("GZIP".to_owned())
                } else {
                    None
                },
            },
        ))
    }

    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<AmazonClientResponse<Vec<u8>>, AmazonClientError> {
        if document.url.contains("expired") {
            return Err(AmazonClientError::ExpiredDownloadUrl);
        }
        let content = if document.url.contains("broken") {
            b"not a valid report".to_vec()
        } else if document.url.contains(marketplace::SALES_AND_TRAFFIC) {
            let query_value = |name: &str| {
                document
                    .url
                    .split_once('?')
                    .and_then(|(_, query)| {
                        query.split('&').find_map(|pair| {
                            let (key, value) = pair.split_once('=')?;
                            (key == name).then_some(value)
                        })
                    })
                    .map(str::to_owned)
            };
            match (query_value("start"), query_value("end")) {
                (Some(start), Some(end)) => {
                    let mut fixture: Value = serde_json::from_str(SALES_FIXTURE)
                        .map_err(|error| AmazonClientError::Permanent(error.to_string()))?;
                    fixture["reportSpecification"]["dataStartTime"] = Value::String(start);
                    fixture["reportSpecification"]["dataEndTime"] = Value::String(end);
                    serde_json::to_vec(&fixture)
                        .map_err(|error| AmazonClientError::Permanent(error.to_string()))?
                }
                _ => SALES_FIXTURE.as_bytes().to_vec(),
            }
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
                .map(|content| fixture_response(AmazonOperation::DownloadReportDocument, content))
                .map_err(|error| AmazonClientError::Permanent(error.to_string()))
        } else {
            Ok(fixture_response(
                AmazonOperation::DownloadReportDocument,
                content,
            ))
        }
    }
}

#[derive(Clone)]
pub struct CompositeAmazonClient {
    live: LiveAmazonClient,
    fixture: FixtureAmazonClient,
}

impl CompositeAmazonClient {
    pub fn new(
        provider_secrets: crate::provider_secrets::ProviderSecretStore,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            live: LiveAmazonClient::new(provider_secrets)?,
            fixture: FixtureAmazonClient,
        })
    }

    fn client(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<&dyn AmazonReportsClient, AmazonClientError> {
        match (
            request.mode.as_str(),
            request.secret_ref.starts_with("fixture:"),
        ) {
            ("fixture", true) => Ok(&self.fixture),
            ("live", false) => Ok(&self.live),
            _ => Err(AmazonClientError::Permanent(
                "Amazon connection mode and transport selector do not match".to_owned(),
            )),
        }
    }
}

#[async_trait]
impl AmazonReportsClient for CompositeAmazonClient {
    async fn create_report(
        &self,
        request: &AmazonReportRequest,
    ) -> Result<AmazonClientResponse<String>, AmazonClientError> {
        self.client(request)?.create_report(request).await
    }

    async fn get_report(
        &self,
        request: &AmazonReportRequest,
        report_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportStatus>, AmazonClientError> {
        self.client(request)?.get_report(request, report_id).await
    }

    async fn get_report_document(
        &self,
        request: &AmazonReportRequest,
        document_id: &str,
    ) -> Result<AmazonClientResponse<AmazonReportDocument>, AmazonClientError> {
        self.client(request)?
            .get_report_document(request, document_id)
            .await
    }

    async fn download_document(
        &self,
        document: &AmazonReportDocument,
    ) -> Result<AmazonClientResponse<Vec<u8>>, AmazonClientError> {
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
}

impl MarketplaceWorker {
    pub fn new(client: Arc<dyn AmazonReportsClient>) -> Self {
        Self { client }
    }

    pub async fn cycle(&self, pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        if !modules::is_enabled(pool, modules::MARKETPLACE_INTELLIGENCE).await?
            || !modules::is_enabled(pool, "intelligence.rules").await?
        {
            return Ok(());
        }
        let pilot_enabled = modules::is_enabled(pool, modules::AMAZON_READ_ONLY_PILOT).await?;
        if !pilot_enabled {
            marketplace::enqueue_due_schedules(pool, 25).await?;
        }
        for run in marketplace::claim_due_runs(pool, 10).await? {
            self.process_run(pool, run).await;
        }
        for job in marketplace::claim_analysis_jobs(pool, 10).await? {
            self.process_analysis(pool, job).await;
        }
        Ok(())
    }

    async fn process_run(&self, pool: &sqlx::PgPool, run: ClaimedReportRun) {
        if !marketplace::transport_mode_is_consistent(&run.mode, &run.secret_ref) {
            let _ = marketplace::mark_run_terminal(
                pool,
                run.id,
                "failed",
                "transport_mode_mismatch",
                "Amazon connection mode and transport selector do not match",
            )
            .await;
            return;
        }
        let request = AmazonReportRequest {
            seller_id: run.seller_id.clone(),
            region: run.region.clone(),
            secret_ref: run.secret_ref.clone(),
            mode: run.mode.clone(),
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
        let pilot_enabled = modules::is_enabled(pool, modules::AMAZON_READ_ONLY_PILOT)
            .await
            .unwrap_or(true);
        if pilot_enabled
            && run.mode == "live"
            && (!pilot_live_run_is_safe(pool, &run).await
                || !pilot_live_sequence_is_safe(pool, &run, Some(run.id))
                    .await
                    .unwrap_or(false))
        {
            let _ = marketplace::mark_run_terminal(
                pool,
                run.id,
                "failed",
                "pilot_live_gate_rejected",
                "Live pilot acquisition must be one manual Sales & Traffic report for a short completed period",
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
                    Ok(response) => {
                        self.record_observations(pool, run.id, &response.observations)
                            .await;
                        let _ = marketplace::set_run_request_created(pool, run.id, &response.value)
                            .await;
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
                    Ok(response) => {
                        self.record_observations(pool, run.id, &response.observations)
                            .await;
                        match response.value {
                            AmazonReportStatus::InProgress => {
                                let delay = exponential_backoff(run.poll_attempts, 60, 3600);
                                let _ = marketplace::set_run_poll_pending(
                                    pool,
                                    run.id,
                                    delay,
                                    "Amazon report is still processing",
                                )
                                .await;
                            }
                            AmazonReportStatus::Done { document_id } => {
                                let _ =
                                    marketplace::set_run_document_ready(pool, run.id, &document_id)
                                        .await;
                            }
                            AmazonReportStatus::Cancelled => {
                                let _ = marketplace::mark_run_terminal(
                                    pool,
                                    run.id,
                                    "cancelled",
                                    "cancelled",
                                    "Amazon cancelled the report or returned no data",
                                )
                                .await;
                            }
                            AmazonReportStatus::Fatal { message } => {
                                let _ = marketplace::mark_run_terminal(
                                    pool, run.id, "fatal", "fatal", &message,
                                )
                                .await;
                            }
                        }
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
                    Ok(document_response) => {
                        self.record_observations(pool, run.id, &document_response.observations)
                            .await;
                        let document = document_response.value;
                        match self.client.download_document(&document).await {
                            Ok(download_response) => {
                                self.record_observations(
                                    pool,
                                    run.id,
                                    &download_response.observations,
                                )
                                .await;
                                let downloaded = download_response.value;
                                match decompress(
                                    &downloaded,
                                    document.compression_algorithm.as_deref(),
                                ) {
                                    Ok(content) => {
                                        let _ = marketplace::archive_document(
                                            pool,
                                            run.id,
                                            &document.document_id,
                                            Some(content_type_for(&run.report_type)),
                                            document.compression_algorithm.as_deref(),
                                            &downloaded,
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
                        }
                    }
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
                observation,
            } => {
                self.record_observations(pool, run_id, &[observation]).await;
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

    async fn record_observations(
        &self,
        pool: &sqlx::PgPool,
        run_id: uuid::Uuid,
        observations: &[AmazonTransportObservation],
    ) {
        for observation in observations {
            if let Err(error) = marketplace::record_transport_observation(
                pool,
                run_id,
                observation.operation.as_str(),
                observation.request_id_redacted.as_deref(),
                observation.rate_limit_limit.as_deref(),
                observation.retry_after_seconds,
            )
            .await
            {
                tracing::warn!(%error, %run_id, "could not persist redacted Amazon transport metadata");
            }
        }
    }

    async fn process_analysis(&self, pool: &sqlx::PgPool, job: ClaimedAnalysisJob) {
        let result = match job.analysis_type.as_str() {
            "delta" => deterministic_delta(pool, &job).await,
            "manual_comparison" => deterministic_manual_comparison(pool, &job).await,
            "total" => deterministic_total(pool, &job).await,
            _ => Err("Unknown analysis type".to_owned()),
        };
        match result {
            Ok(mut result) => {
                let payload = pii_safe_analysis_export(&result);
                result["analysis_engine"] = json!({
                    "kind": "deterministic_rules",
                    "ruleset_version": RULESET_VERSION,
                    "external_ai": false,
                });
                let _ = marketplace::complete_analysis(
                    pool,
                    &job,
                    "deterministic_rules",
                    None,
                    RULESET_VERSION,
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

async fn staging_context_is_approved(
    pool: &sqlx::PgPool,
    seller_id: &str,
    region: &str,
    secret_ref: &str,
    marketplace_id: &str,
) -> bool {
    let environment_approved = std::env::var("AMAZON_STAGING_APPROVAL")
        .ok()
        .and_then(|raw| serde_json::from_str::<AmazonStagingApproval>(&raw).ok())
        .is_some_and(|approval| {
            marketplace::live_secret_reference_is_configured(secret_ref)
                && approval.seller_sha256.len() == 64
                && approval
                    .seller_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                && approval.seller_sha256 == sha256(seller_id.as_bytes())
                && approval.region == region
                && approval.marketplace_id == marketplace_id
        });
    if environment_approved {
        return true;
    }
    if secret_ref != db::provider_secrets::PILOT_AMAZON_SECRET_REF {
        return false;
    }
    let context_sha256 =
        crate::provider_secrets::amazon_context_sha256(seller_id, region, marketplace_id);
    db::provider_secrets::amazon_context_is_approved(pool, &context_sha256)
        .await
        .unwrap_or(false)
}

fn pilot_period_is_safe(
    report_type: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    report_options: &Value,
) -> bool {
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };
    let period_days = (end.date_naive() - start.date_naive()).num_days() + 1;
    report_type == marketplace::SALES_AND_TRAFFIC
        && start < end
        && end.date_naive() < Utc::now().date_naive()
        && (1..=7).contains(&period_days)
        && *report_options == json!({"dateGranularity": "DAY", "asinGranularity": "CHILD"})
}

async fn pilot_live_run_is_safe(pool: &sqlx::PgPool, run: &ClaimedReportRun) -> bool {
    run.trigger_source == "manual"
        && run.schedule_id.is_none()
        && staging_context_is_approved(
            pool,
            &run.seller_id,
            &run.region,
            &run.secret_ref,
            &run.marketplace_id,
        )
        .await
        && pilot_period_is_safe(
            &run.report_type,
            run.data_start_time,
            run.data_end_time,
            &run.report_options,
        )
}

async fn pilot_live_sequence_is_safe(
    pool: &sqlx::PgPool,
    run: &ClaimedReportRun,
    exclude_run_id: Option<uuid::Uuid>,
) -> Result<bool, sqlx::Error> {
    let other_active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM amazon_report_runs
         WHERE connection_id = $1 AND ($2::uuid IS NULL OR id <> $2)
           AND status IN ('queued', 'requesting', 'polling', 'downloading', 'parsing', 'analysing')",
    )
    .bind(run.connection_id)
    .bind(exclude_run_id)
    .fetch_one(pool)
    .await?;
    if other_active != 0 {
        return Ok(false);
    }
    let prior_comparability_key = sqlx::query_scalar::<_, String>(
        "SELECT snapshot.comparability_key
         FROM amazon_metric_snapshots snapshot
         JOIN amazon_report_runs prior_run ON prior_run.id = snapshot.run_id
         WHERE snapshot.connection_id = $1 AND snapshot.marketplace_id = $2
           AND snapshot.report_type = $3 AND snapshot.parser_version = 'sales-traffic-json-v2'
           AND snapshot.granularity = 'day_child' AND prior_run.status = 'succeeded'
         ORDER BY prior_run.completed_at DESC LIMIT 1",
    )
    .bind(run.connection_id)
    .bind(&run.marketplace_id)
    .bind(&run.report_type)
    .fetch_optional(pool)
    .await?;
    let Some(prior_comparability_key) = prior_comparability_key else {
        return Ok(true);
    };
    let period_days = match (run.data_start_time, run.data_end_time) {
        (Some(start), Some(end)) => (end.date_naive() - start.date_naive()).num_days() + 1,
        _ => return Ok(false),
    };
    Ok(prior_comparability_key == format!("sales-traffic:day_child:{period_days}d"))
}

pub(crate) async fn pilot_live_request_is_safe(
    pool: &sqlx::PgPool,
    connection: &marketplace::AmazonConnection,
    input: &marketplace::CreateAmazonReportRunInput,
) -> Result<bool, sqlx::Error> {
    let run = ClaimedReportRun {
        id: uuid::Uuid::nil(),
        connection_id: connection.id,
        schedule_id: None,
        marketplace_id: input.marketplace_id.clone(),
        report_type: input.report_type.clone(),
        data_start_time: input.data_start_time,
        data_end_time: input.data_end_time,
        report_options: input.report_options.clone(),
        trigger_source: "manual".to_owned(),
        status: "queued".to_owned(),
        attempts: 0,
        poll_attempts: 0,
        amazon_report_id: None,
        amazon_report_document_id: None,
        seller_id: connection.seller_id.clone(),
        region: connection.region.clone(),
        secret_ref: connection.secret_ref.clone(),
        granted_roles: connection.granted_roles.clone(),
        mode: connection.mode.clone(),
    };
    Ok(pilot_live_run_is_safe(pool, &run).await
        && pilot_live_sequence_is_safe(pool, &run, None).await?)
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
    let specification = value.get("reportSpecification");
    let report_options = specification.and_then(|specification| specification.get("reportOptions"));
    let date_granularity = report_options
        .and_then(|options| options.get("dateGranularity"))
        .and_then(Value::as_str)
        .unwrap_or("DAY");
    let asin_granularity = report_options
        .and_then(|options| options.get("asinGranularity"))
        .and_then(Value::as_str)
        .or_else(|| infer_asin_granularity(&rows))
        .unwrap_or("PARENT");
    if !matches!(date_granularity, "DAY" | "WEEK" | "MONTH")
        || !matches!(asin_granularity, "PARENT" | "CHILD" | "SKU")
    {
        return Err("Sales & Traffic report has unsupported granularity".to_owned());
    }
    let date_granularity_key = date_granularity.to_ascii_lowercase();
    let asin_granularity_key = asin_granularity.to_ascii_lowercase();
    let mut metrics = Vec::new();
    let mut total_sales = Decimal::ZERO;
    let mut total_units = Decimal::ZERO;
    let mut total_sessions = Decimal::ZERO;
    let mut total_page_views = Decimal::ZERO;
    let mut sessions_present = false;
    let mut page_views_present = false;
    let mut currencies = BTreeMap::<String, Decimal>::new();
    let mut dates = Vec::new();
    let mut dimensions = HashSet::new();
    for (row, row_date) in rows {
        let asin = row
            .get(match asin_granularity {
                "PARENT" => "parentAsin",
                "CHILD" => "childAsin",
                "SKU" => "sku",
                _ => unreachable!(),
            })
            .or_else(|| row.get("asin"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("Sales & Traffic row lacks required {asin_granularity} identifier")
            })?;
        if let Some(date) = row_date {
            dates.push(date);
        }
        let dimension = format!(
            "{asin}:{}",
            row_date.map_or_else(|| "aggregate".to_owned(), |date| date.to_string())
        );
        if !dimensions.insert(dimension) {
            return Err(format!(
                "Sales & Traffic report contains duplicate ASIN/date row for {asin}"
            ));
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
        let sessions = optional_decimal(value_at(row, &["trafficByAsin", "sessions"]));
        let page_views = optional_decimal(value_at(row, &["trafficByAsin", "pageViews"]));
        total_sales += sales;
        total_units += units;
        if let Some(sessions) = sessions {
            sessions_present = true;
            total_sessions += sessions;
        }
        if let Some(page_views) = page_views {
            page_views_present = true;
            total_page_views += page_views;
        }
        *currencies.entry(currency.clone()).or_default() += sales;
        let evidence = json!({
            "dimension": asin,
            "dimension_kind": asin_granularity_key,
            "date": row_date.map(|value| value.to_string()),
        });
        let asin_dimension_key = row_date
            .map(|date| format!("{asin}:{date}"))
            .unwrap_or_else(|| asin.to_owned());
        metrics.extend([
            ParsedMetric {
                metric_name: "ordered_product_sales".to_owned(),
                dimension_type: format!("{asin_granularity_key}_period"),
                dimension_key: asin_dimension_key.clone(),
                value_numeric: sales,
                unit: "currency".to_owned(),
                currency_code: Some(currency.clone()),
                evidence: evidence.clone(),
            },
            ParsedMetric {
                metric_name: "units_ordered".to_owned(),
                dimension_type: format!("{asin_granularity_key}_period"),
                dimension_key: asin_dimension_key.clone(),
                value_numeric: units,
                unit: "units".to_owned(),
                currency_code: None,
                evidence: evidence.clone(),
            },
        ]);
        if let Some(sessions) = sessions {
            metrics.push(ParsedMetric {
                metric_name: "sessions".to_owned(),
                dimension_type: format!("{asin_granularity_key}_period"),
                dimension_key: asin_dimension_key.clone(),
                value_numeric: sessions,
                unit: "sessions".to_owned(),
                currency_code: None,
                evidence: evidence.clone(),
            });
        }
        if let Some(page_views) = page_views {
            metrics.push(ParsedMetric {
                metric_name: "page_views".to_owned(),
                dimension_type: format!("{asin_granularity_key}_period"),
                dimension_key: asin_dimension_key,
                value_numeric: page_views,
                unit: "views".to_owned(),
                currency_code: None,
                evidence,
            });
        }
    }
    if currencies.len() > 1 {
        return Err(
            "Sales & Traffic report contains multiple currencies; no silent aggregation".to_owned(),
        );
    }
    let currency = currencies.keys().next().cloned();
    let report_start = specification
        .and_then(|value| value.get("dataStartTime"))
        .and_then(Value::as_str)
        .and_then(parse_amazon_date);
    let report_end = specification
        .and_then(|value| value.get("dataEndTime"))
        .and_then(Value::as_str)
        .and_then(parse_amazon_date);
    let start = dates
        .iter()
        .min()
        .copied()
        .map(|date| Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap()))
        .or(report_start)
        .or(requested_start);
    let end = dates
        .iter()
        .max()
        .copied()
        .map(|date| Utc.from_utc_datetime(&date.and_hms_opt(23, 59, 59).unwrap()))
        .or(report_end)
        .or(requested_end);
    let period_days = match (start, end) {
        (Some(start), Some(end)) => (end.date_naive() - start.date_naive()).num_days() + 1,
        _ => 0,
    };
    let conversion = if !sessions_present || total_sessions.is_zero() {
        None
    } else {
        Some((total_units / total_sessions * Decimal::from(100)).round_dp(4))
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
    ]);
    if sessions_present {
        metrics.push(ParsedMetric {
            metric_name: "sessions".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_sessions,
            unit: "sessions".to_owned(),
            currency_code: None,
            evidence: json!({ "aggregation": "sum_by_asin" }),
        });
    }
    if page_views_present {
        metrics.push(ParsedMetric {
            metric_name: "page_views".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_page_views,
            unit: "views".to_owned(),
            currency_code: None,
            evidence: json!({ "aggregation": "sum_by_asin" }),
        });
    }
    if let Some(conversion) = conversion {
        metrics.push(ParsedMetric {
            metric_name: "conversion_rate".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: conversion,
            unit: "percent".to_owned(),
            currency_code: None,
            evidence: json!({ "formula": "units_ordered / sessions * 100" }),
        });
    }
    Ok(ParsedSnapshot {
        parser_version: "sales-traffic-json-v2".to_owned(),
        period_start: start,
        period_end: end,
        granularity: format!("{date_granularity_key}_{asin_granularity_key}"),
        comparability_key: format!(
            "sales-traffic:{date_granularity_key}_{asin_granularity_key}:{period_days}d"
        ),
        summary: json!({
            "ordered_product_sales": total_sales.to_string(),
            "currency": currency,
            "units_ordered": total_units.to_string(),
            "sessions": sessions_present.then(|| total_sessions.to_string()),
            "page_views": page_views_present.then(|| total_page_views.to_string()),
            "conversion_rate": conversion.map(|value| value.to_string()),
            "period_days": period_days,
            "date_granularity": date_granularity,
            "asin_granularity": asin_granularity,
        }),
        metrics,
    })
}

fn infer_asin_granularity(rows: &[(&Value, Option<NaiveDate>)]) -> Option<&'static str> {
    let first = rows.first()?.0;
    if first.get("sku").and_then(Value::as_str).is_some() {
        Some("SKU")
    } else if first.get("childAsin").and_then(Value::as_str).is_some() {
        Some("CHILD")
    } else if first.get("parentAsin").and_then(Value::as_str).is_some() {
        Some("PARENT")
    } else {
        None
    }
}

fn parse_amazon_date(value: &str) -> Option<DateTime<Utc>> {
    NaiveDate::parse_from_str(value.get(..10).unwrap_or(value), "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|date| Utc.from_utc_datetime(&date))
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
    let mut seen_skus = HashSet::new();
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
        if !seen_skus.insert(sku.to_owned()) {
            return Err(format!(
                "Inventory Planning report contains duplicate sku {sku}"
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
    metrics.push(ParsedMetric {
        metric_name: "available_inventory".to_owned(),
        dimension_type: "catalog".to_owned(),
        dimension_key: String::new(),
        value_numeric: total_available,
        unit: "units".to_owned(),
        currency_code: None,
        evidence: json!({ "aggregation": "sum_by_sku" }),
    });
    if shipped_index.is_some() {
        metrics.push(ParsedMetric {
            metric_name: "units_shipped_t30".to_owned(),
            dimension_type: "catalog".to_owned(),
            dimension_key: String::new(),
            value_numeric: total_shipped,
            unit: "units_30d".to_owned(),
            currency_code: None,
            evidence: json!({ "aggregation": "sum_by_sku" }),
        });
    }
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
            "units_shipped_t30": shipped_index.map(|_| total_shipped.to_string()),
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

fn analysis_context(snapshot: &MetricSnapshot) -> Value {
    json!({
        "period_start": snapshot.period_start,
        "period_end": snapshot.period_end,
        "marketplace": snapshot.marketplace_id,
        "report_type": snapshot.report_type,
        "granularity": snapshot.granularity,
        "parser_version": snapshot.parser_version,
        "data_freshness": snapshot.summary.get("data_freshness").cloned()
            .or_else(|| snapshot.period_end.map(|value| json!(value)))
            .unwrap_or(Value::Null),
        "missing_fields": snapshot.summary.get("missing_fields").cloned().unwrap_or_else(|| json!([])),
        "source_timezone": snapshot.summary.get("timezone").cloned()
            .or_else(|| snapshot.summary.get("reporting_timezone").cloned())
            .unwrap_or(Value::Null),
        "currency": snapshot.summary.get("currency_code").cloned()
            .or_else(|| snapshot.summary.get("currency").cloned())
            .unwrap_or(Value::Null),
    })
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
    let previous = marketplace::previous_compatible_snapshot(pool, &current)
        .await
        .map_err(|error| error.to_string())?;
    deterministic_delta_for_snapshots(pool, &current, previous).await
}

async fn deterministic_manual_comparison(
    pool: &sqlx::PgPool,
    job: &ClaimedAnalysisJob,
) -> Result<Value, String> {
    let uploaded_run_id = job
        .run_id
        .ok_or_else(|| "Manual comparison requires an uploaded report run".to_owned())?;
    let anchor = marketplace::snapshot_for_run(pool, uploaded_run_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Manual comparison lacks its uploaded snapshot".to_owned())?;
    let snapshots = marketplace::snapshots_for_window(pool, job)
        .await
        .map_err(|error| error.to_string())?;
    let expected_start = job
        .period_start
        .ok_or_else(|| "Manual comparison lacks a period start".to_owned())?;
    let expected_end = job
        .period_end
        .ok_or_else(|| "Manual comparison lacks a period end".to_owned())?;
    let previous = snapshots
        .iter()
        .find(|snapshot| {
            snapshot.comparability_key == anchor.comparability_key
                && snapshot.parser_version == anchor.parser_version
                && snapshot.period_start == Some(expected_start)
        })
        .cloned()
        .ok_or_else(|| "Manual comparison lacks its earlier compatible snapshot".to_owned())?;
    let current = snapshots
        .iter()
        .rev()
        .find(|snapshot| {
            snapshot.comparability_key == anchor.comparability_key
                && snapshot.parser_version == anchor.parser_version
                && snapshot.period_end == Some(expected_end)
        })
        .cloned()
        .ok_or_else(|| "Manual comparison lacks its later compatible snapshot".to_owned())?;
    let previous_end = previous
        .period_end
        .ok_or_else(|| "Manual comparison earlier period is incomplete".to_owned())?;
    let current_start = current
        .period_start
        .ok_or_else(|| "Manual comparison later period is incomplete".to_owned())?;
    if previous.id == current.id || previous_end >= current_start {
        return Err("Manual comparison periods overlap or are identical".to_owned());
    }
    deterministic_delta_for_snapshots(pool, &current, Some(previous)).await
}

async fn deterministic_delta_for_snapshots(
    pool: &sqlx::PgPool,
    current: &MetricSnapshot,
    previous: Option<MetricSnapshot>,
) -> Result<Value, String> {
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
                "evidence_ref": evidence_ref(current, metric),
            })
        })
        .collect::<Vec<_>>();
    let context = analysis_context(current);
    let Some(previous) = previous else {
        return Ok(json!({
            "context": context,
            "facts": facts,
            "derived_observations": [
                "The current report was normalized successfully; no compatible earlier period is available for a deterministic delta."
            ],
            "changes_since_last_run": [],
            "overall_trend": "No comparable earlier snapshot is available.",
            "anomalies": [],
            "hypotheses": [],
            "options": generic_options(),
            "uncertainty": "high",
            "missing_data": ["A previous successful snapshot with matching report type, granularity and period length is required for a delta analysis."],
            "missing_evidence": ["A second non-overlapping period with identical marketplace, report type, granularity, parser version, currency and period length."],
            "open_questions": ["Which earlier period should be imported for comparison?"],
            "recommendation_notice": "Recommendations only; Essentials+ Merchant does not make Amazon changes.",
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
        let trend = match percentage {
            Some(value) if value > Decimal::ONE => "up",
            Some(value) if value < -Decimal::ONE => "down",
            Some(_) => "stable",
            None if difference.is_zero() => "stable",
            None if difference.is_sign_positive() => "up_from_zero",
            None => "down_to_zero",
        };
        let change = json!({
            "metric": metric.metric_name,
            "current": metric.value_numeric.to_string(),
            "previous": previous_metric.value_numeric.to_string(),
            "difference": difference.to_string(),
            "percent_change": percentage.map(|value| value.to_string()),
            "trend": trend,
            "unit": metric.unit,
            "currency": metric.currency_code,
            "evidence_refs": [evidence_ref(current, metric), evidence_ref(&previous, previous_metric)],
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
    let derived_observations = changes
        .iter()
        .map(|change| {
            json!({
                "kind": "deterministic_period_delta",
                "metric": change.get("metric"),
                "delta": change.get("difference"),
                "percent_change": change.get("percent_change"),
                "trend": change.get("trend"),
                "evidence_refs": change.get("evidence_refs"),
            })
        })
        .collect::<Vec<_>>();
    let hypotheses = hypotheses_for_changes(&changes);
    Ok(json!({
        "context": context,
        "facts": facts,
        "derived_observations": derived_observations,
        "changes_since_last_run": changes,
        "overall_trend": trend,
        "anomalies": anomalies,
        "hypotheses": hypotheses,
        "options": options,
        "uncertainty": if anomalies.is_empty() { "medium" } else { "medium; material changes need operational validation" },
        "missing_data": missing_data_for_metrics(&current_metrics),
        "missing_evidence": [
            "Price, promotion, advertising, listing, availability and competitor changes are not contained in Sales and Traffic reports.",
            "The report supports correlation and deterministic deltas, not causal attribution."
        ],
        "open_questions": [
            "Were price, promotions, advertising, availability or listing content changed between the periods?",
            "Was Seller Central report coverage complete and final for both periods?"
        ],
        "recommendation_notice": "Recommendations only; Essentials+ Merchant does not make Amazon changes.",
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
            "recommendation_notice": "Recommendations only; Essentials+ Merchant does not make Amazon changes.",
        }));
    }
    let keys = snapshots
        .iter()
        .map(|snapshot| (&snapshot.comparability_key, &snapshot.parser_version))
        .collect::<std::collections::BTreeSet<_>>();
    if keys.len() != 1 {
        return Ok(json!({
            "facts": [], "changes_since_last_run": [], "overall_trend": "Analysis was not aggregated.",
            "anomalies": [], "hypotheses": [], "options": generic_options(), "uncertainty": "high",
            "missing_data": ["Selected snapshots have incompatible granularity or reporting periods and are intentionally not compared."],
            "recommendation_notice": "Recommendations only; Essentials+ Merchant does not make Amazon changes.",
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
        "recommendation_notice": "Recommendations only; Essentials+ Merchant does not make Amazon changes.",
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
            "action": "Import another completed, compatible comparison period",
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
        matches!(
            metric.metric_name.as_str(),
            "conversion_rate" | "unit_session_percentage"
        ) && metric.dimension_type == "catalog"
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
    if let Some(traffic) = metrics
        .iter()
        .find(|metric| metric.metric_name == "sessions" && metric.dimension_type == "catalog")
    {
        options.push(json!({
            "action": "Review traffic-source and discoverability evidence for the same periods",
            "expected_effect": "Helps distinguish a traffic shift from a conversion or reporting-coverage shift.",
            "effort": "low",
            "risks": ["Traffic correlation alone does not establish the cause."],
            "evidence_refs": [format!("snapshot:{}:metric:{}", traffic.snapshot_id, traffic.id)],
            "uncertainty": "medium",
        }));
    }
    if let Some(buy_box) = metrics.iter().find(|metric| {
        metric.metric_name == "buy_box_percentage" && metric.dimension_type == "catalog"
    }) {
        options.push(json!({
            "action": "Review offer, fulfilment and availability history for the comparison periods",
            "expected_effect": "May explain a Buy Box or conversion movement without changing Amazon automatically.",
            "effort": "medium",
            "risks": ["The Sales and Traffic report does not contain all offer-level causes."],
            "evidence_refs": [format!("snapshot:{}:metric:{}", buy_box.snapshot_id, buy_box.id)],
            "uncertainty": "high",
        }));
    }
    options.truncate(5);
    options
}

fn hypotheses_for_changes(changes: &[Value]) -> Vec<Value> {
    changes
        .iter()
        .filter_map(|change| {
            let metric = change.get("metric")?.as_str()?;
            let hypothesis = match metric {
                "ordered_product_sales" => {
                    "Revenue may have moved with traffic, conversion, Buy Box, assortment, availability or report coverage."
                }
                "sessions" | "page_views" => {
                    "Traffic acquisition, organic discoverability, seasonality or report coverage may have changed."
                }
                "conversion_rate" | "unit_session_percentage" => {
                    "Offer competitiveness, listing quality, availability or traffic mix may have changed conversion."
                }
                "buy_box_percentage" => {
                    "Offer competitiveness, fulfilment eligibility or availability may have affected Buy Box share."
                }
                "b2b_share"
                | "b2b_revenue_share"
                | "b2b_units_share"
                | "b2b_ordered_product_sales"
                | "b2b_units_ordered" => {
                    "The mix of business-customer demand may have changed between periods."
                }
                _ => return None,
            };
            Some(json!({
                "hypothesis": hypothesis,
                "metric": metric,
                "evidence_refs": change.get("evidence_refs").cloned().unwrap_or_else(|| json!([])),
                "uncertainty": "high; Sales and Traffic reports do not establish causality.",
            }))
        })
        .take(5)
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
    if !names.contains(&"buy_box_percentage") {
        missing.push("Buy Box percentage is not present in the imported report.");
    }
    if !names.contains(&"b2b_share")
        && !names.contains(&"b2b_revenue_share")
        && !names.contains(&"b2b_units_share")
        && !names.contains(&"b2b_ordered_product_sales")
        && !names.contains(&"b2b_units_ordered")
    {
        missing.push("B2B sales or unit share is not present in the imported report.");
    }
    missing
}

/// Export only aggregate allowlisted metrics. This is the downloadable-summary
/// boundary; the optional AI strategy integration applies an additional closed
/// DTO that also removes internal evidence identifiers.
pub fn pii_safe_analysis_export(result: &Value) -> Value {
    let allowed = [
        "ordered_product_sales",
        "units_ordered",
        "sessions",
        "page_views",
        "conversion_rate",
        "unit_session_percentage",
        "buy_box_percentage",
        "b2b_share",
        "b2b_revenue_share",
        "b2b_units_share",
        "b2b_ordered_product_sales",
        "b2b_units_ordered",
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
        .map(strip_pii)
        .collect::<Vec<_>>();
    let mut export = serde_json::Map::new();
    if let Some(context) = result.get("context").and_then(Value::as_object) {
        let allowed_context = [
            "period_start",
            "period_end",
            "marketplace",
            "report_type",
            "granularity",
            "parser_version",
            "data_freshness",
            "missing_fields",
            "source_timezone",
            "currency",
        ];
        export.insert(
            "context".to_owned(),
            Value::Object(
                context
                    .iter()
                    .filter(|(key, _)| allowed_context.contains(&key.as_str()))
                    .map(|(key, value)| (key.clone(), strip_pii(value)))
                    .collect(),
            ),
        );
    }
    export.insert("facts".to_owned(), Value::Array(facts));
    for field in [
        "derived_observations",
        "changes_since_last_run",
        "overall_trend",
        "seasonality",
        "anomalies",
        "hypotheses",
        "options",
        "uncertainty",
        "missing_data",
        "missing_evidence",
        "open_questions",
        "recommendation_notice",
        "analysis_engine",
    ] {
        if let Some(value) = result.get(field) {
            export.insert(field.to_owned(), strip_pii(value));
        }
    }
    Value::Object(export)
}

fn strip_pii(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .filter(|(key, _)| {
                    let key = key.to_ascii_lowercase();
                    ![
                        "buyer",
                        "customer",
                        "email",
                        "address",
                        "order_id",
                        "comment",
                        "phone",
                        "person",
                        "asin",
                        "sku",
                        "seller_id",
                        "merchant_id",
                        "raw",
                        "path",
                    ]
                    .iter()
                    .any(|forbidden| key.contains(forbidden))
                })
                .map(|(key, value)| (key.clone(), strip_pii(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(strip_pii).collect()),
        Value::String(value) if value.contains('@') => Value::String("[redacted]".to_owned()),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pilot_operation_contract_is_exact_and_download_hosts_fail_closed() {
        assert_eq!(
            AMAZON_PILOT_OPERATION_ALLOWLIST
                .iter()
                .map(|operation| operation.as_str())
                .collect::<Vec<_>>(),
            vec![
                "lwa_token_refresh",
                "create_report",
                "get_report",
                "get_report_document",
                "download_report_document",
            ]
        );
        assert!(download_url_allowed(
            "https://example.s3.eu-central-1.amazonaws.com/report?signature=redacted"
        ));
        assert!(!download_url_allowed(
            "https://example.invalid/report?signature=redacted"
        ));
        assert!(!download_url_allowed("file:///tmp/report"));
    }

    #[test]
    fn pilot_live_period_requires_completed_utc_days_and_exact_sales_options() {
        let yesterday = Utc::now().date_naive().pred_opt().unwrap();
        let start =
            DateTime::from_naive_utc_and_offset(yesterday.and_hms_opt(0, 0, 0).unwrap(), Utc);
        let end =
            DateTime::from_naive_utc_and_offset(yesterday.and_hms_opt(23, 59, 59).unwrap(), Utc);
        let options = json!({"dateGranularity": "DAY", "asinGranularity": "CHILD"});
        assert!(pilot_period_is_safe(
            marketplace::SALES_AND_TRAFFIC,
            Some(start),
            Some(end),
            &options,
        ));
        assert!(!pilot_period_is_safe(
            marketplace::INVENTORY_PLANNING,
            Some(start),
            Some(end),
            &options,
        ));
        assert!(!pilot_period_is_safe(
            marketplace::SALES_AND_TRAFFIC,
            Some(start),
            Some(Utc::now()),
            &options,
        ));
        assert!(!pilot_period_is_safe(
            marketplace::SALES_AND_TRAFFIC,
            Some(start),
            Some(end),
            &json!({"dateGranularity": "DAY"}),
        ));
    }

    #[tokio::test]
    async fn live_transport_contract_runs_against_local_fake_sp_api_only() {
        use axum::extract::{Path, State};
        use axum::http::HeaderMap;
        use axum::routing::{get, post};
        use axum::{Json, Router};

        async fn token() -> Json<Value> {
            Json(json!({ "access_token": "synthetic-access-token" }))
        }
        async fn create(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers
                    .get("x-amz-access-token")
                    .and_then(|value| value.to_str().ok()),
                Some("synthetic-access-token")
            );
            assert_eq!(body["reportType"], marketplace::SALES_AND_TRAFFIC);
            assert_eq!(body["marketplaceIds"][0], "A1PA6795UKMFR9");
            Json(json!({ "reportId": "fake-report-1" }))
        }
        async fn report(Path(report_id): Path<String>) -> Json<Value> {
            assert_eq!(report_id, "fake-report-1");
            Json(json!({
                "processingStatus": "DONE",
                "reportDocumentId": "fake-document-1",
            }))
        }
        async fn document(
            State(base): State<String>,
            Path(document_id): Path<String>,
        ) -> Json<Value> {
            assert_eq!(document_id, "fake-document-1");
            Json(json!({ "url": format!("{base}/download") }))
        }
        async fn download() -> &'static [u8] {
            SALES_FIXTURE.as_bytes()
        }
        async fn partial() -> axum::response::Response {
            axum::response::Response::builder()
                .header(axum::http::header::CONTENT_LENGTH, "100")
                .body(axum::body::Body::from("truncated"))
                .unwrap()
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/auth/o2/token", post(token))
            .route("/reports/2021-06-30/reports", post(create))
            .route("/reports/2021-06-30/reports/{report_id}", get(report))
            .route("/reports/2021-06-30/documents/{document_id}", get(document))
            .route("/download", get(download))
            .route("/partial", get(partial))
            .with_state(base.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = LiveAmazonClient::for_fake_server(base).unwrap();
        let request = AmazonReportRequest {
            seller_id: "SYNTHETIC-SELLER".to_owned(),
            region: "eu".to_owned(),
            secret_ref: "fake-server".to_owned(),
            mode: "live".to_owned(),
            report_type: marketplace::SALES_AND_TRAFFIC.to_owned(),
            marketplace_id: "A1PA6795UKMFR9".to_owned(),
            data_start_time: None,
            data_end_time: None,
            report_options: json!({}),
        };
        let report_id = client.create_report(&request).await.unwrap().value;
        let AmazonReportStatus::Done { document_id } =
            client.get_report(&request, &report_id).await.unwrap().value
        else {
            panic!("fake report must be DONE");
        };
        let document = client
            .get_report_document(&request, &document_id)
            .await
            .unwrap()
            .value;
        let bytes = client.download_document(&document).await.unwrap().value;
        assert_eq!(bytes, SALES_FIXTURE.as_bytes());
        let partial = client
            .download_document(&AmazonReportDocument {
                document_id: "partial-document".to_owned(),
                url: format!("{}/partial", client.endpoint("eu").unwrap()),
                compression_algorithm: None,
            })
            .await;
        assert!(matches!(partial, Err(AmazonClientError::Retryable(_))));
        server.abort();
    }

    #[test]
    fn parses_sales_json_without_binary_money() {
        let parsed = parse_sales_and_traffic(SALES_FIXTURE.as_bytes(), None, None).unwrap();
        assert_eq!(parsed.parser_version, "sales-traffic-json-v2");
        assert_eq!(parsed.granularity, "day_child");
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
    fn parsers_handle_optional_columns_order_empty_duplicates_and_encoding_explicitly() {
        let reordered = b"available\textra-new\tsku\n4\tignored\tSKU-OPTIONAL\n";
        let parsed = parse_inventory_planning(reordered).unwrap();
        assert_eq!(parsed.summary["available_inventory"], "4");
        assert!(parsed.summary["units_shipped_t30"].is_null());
        assert!(!parsed
            .metrics
            .iter()
            .any(|metric| metric.metric_name == "units_shipped_t30"));
        assert!(parse_inventory_planning(b"sku\tavailable\n").is_err());
        assert!(parse_inventory_planning(b"sku\tavailable\nDUP\t1\nDUP\t1\n").is_err());
        assert!(parse_inventory_planning(&[0xff, 0xfe, 0xfd]).is_err());
        assert!(parse_sales_and_traffic(br#"{"salesAndTrafficByDate":[]}"#, None, None).is_err());
        let duplicate_sales = br#"{"salesAndTrafficByAsin":[
          {"childAsin":"B0DUPLICATE","salesByAsin":{"orderedProductSales":{"amount":"1.00","currencyCode":"EUR"},"unitsOrdered":1}},
          {"childAsin":"B0DUPLICATE","salesByAsin":{"orderedProductSales":{"amount":"1.00","currencyCode":"EUR"},"unitsOrdered":1}}
        ]}"#;
        assert!(parse_sales_and_traffic(duplicate_sales, None, None).is_err());
    }

    #[test]
    fn missing_optional_traffic_is_not_silently_normalized_to_zero() {
        let fixture = br#"{
          "salesAndTrafficByAsin": [{
            "childAsin":"B0OPTIONAL1",
            "salesByAsin":{"orderedProductSales":{"amount":"10.00","currencyCode":"EUR"},"unitsOrdered":1},
            "unknownFutureField":"accepted"
          }]
        }"#;
        let parsed = parse_sales_and_traffic(fixture, None, None).unwrap();
        assert!(parsed.summary["sessions"].is_null());
        assert!(parsed.summary["conversion_rate"].is_null());
        assert!(!parsed
            .metrics
            .iter()
            .any(|metric| matches!(metric.metric_name.as_str(), "sessions" | "conversion_rate")));
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
    fn analysis_export_removes_unallowlisted_and_pii_fields() {
        let payload = pii_safe_analysis_export(&json!({
            "facts": [
                { "metric": "sessions", "value": "10" },
                { "metric": "buyer_email", "value": "private@example.test" }
            ],
            "options": [{
                "action": "Review aggregate trend",
                "buyer_email": "private@example.test",
                "evidence_refs": ["snapshot:synthetic:metric:1"]
            }]
        }));
        assert_eq!(payload["facts"].as_array().unwrap().len(), 1);
        assert!(payload["options"][0].get("buyer_email").is_none());
        assert!(!payload.to_string().contains("private@example.test"));
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
            MarketplaceWorker::new(Arc::new(FixtureAmazonClient))
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
            detail.analyses[0].result["analysis_engine"]["external_ai"],
            false
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
    async fn inconsistent_fixture_mode_is_failed_before_any_transport(pool: sqlx::PgPool) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let connection_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO amazon_connections
                 (seller_id, region, secret_ref, granted_roles, mode)
             VALUES ('SYNTHETIC-MISMATCH', 'eu', 'pilot_seller',
                     ARRAY['Brand Analytics'], 'fixture')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let run_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO amazon_report_runs
                 (connection_id, marketplace_id, report_type, report_options,
                  trigger_source, idempotency_key, status)
             VALUES ($1, 'A1PA6795UKMFR9', $2,
                     '{\"dateGranularity\":\"DAY\",\"asinGranularity\":\"CHILD\"}',
                     'manual', 'synthetic-mode-mismatch', 'queued')
             RETURNING id",
        )
        .bind(connection_id)
        .bind(marketplace::SALES_AND_TRAFFIC)
        .fetch_one(&pool)
        .await
        .unwrap();

        MarketplaceWorker::new(Arc::new(FixtureAmazonClient))
            .cycle(&pool)
            .await
            .unwrap();
        let detail = db::marketplace::get_run_detail(&pool, run_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.run.status, "failed");
        assert_eq!(
            detail.run.failure_code.as_deref(),
            Some("transport_mode_mismatch")
        );
        assert!(detail.document.is_none());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn parser_versions_and_period_keys_are_required_for_snapshot_comparison(
        pool: sqlx::PgPool,
    ) {
        let connection = db::marketplace::create_demo_connection(&pool)
            .await
            .unwrap();
        let mut snapshots = Vec::new();
        for (index, parser_version, key) in [
            (1, "parser-v1", "same-period"),
            (2, "parser-v2", "different-period"),
            (3, "parser-v2", "same-period"),
        ] {
            let run_id: uuid::Uuid = sqlx::query_scalar(
                "INSERT INTO amazon_report_runs (
                     connection_id, marketplace_id, report_type, trigger_source,
                     idempotency_key, status, completed_at
                 ) VALUES ($1, 'A1PA6795UKMFR9', $2, 'manual', $3, 'succeeded', now())
                 RETURNING id",
            )
            .bind(connection.id)
            .bind(marketplace::SALES_AND_TRAFFIC)
            .bind(format!("comparison-{index}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            let snapshot: MetricSnapshot = sqlx::query_as(
                "INSERT INTO amazon_metric_snapshots (
                     run_id, connection_id, marketplace_id, report_type, parser_version,
                     granularity, comparability_key, summary, created_at
                 ) VALUES ($1, $2, 'A1PA6795UKMFR9', $3, $4, 'daily_asin', $5, '{}',
                     now() + ($6::text || ' seconds')::interval)
                 RETURNING id, run_id, connection_id, marketplace_id, report_type,
                     parser_version, period_start, period_end, granularity,
                     comparability_key, summary, created_at",
            )
            .bind(run_id)
            .bind(connection.id)
            .bind(marketplace::SALES_AND_TRAFFIC)
            .bind(parser_version)
            .bind(key)
            .bind(index)
            .fetch_one(&pool)
            .await
            .unwrap();
            snapshots.push(snapshot);
        }
        let previous = db::marketplace::previous_compatible_snapshot(&pool, &snapshots[2])
            .await
            .unwrap();
        assert!(
            previous.is_none(),
            "neither another parser version nor another period key is compatible"
        );
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn compatible_snapshots_produce_an_evidence_backed_delta(pool: sqlx::PgPool) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let connection = db::marketplace::create_demo_connection(&pool)
            .await
            .unwrap();
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient));
        let periods = [
            (
                Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 7, 7, 23, 59, 59).unwrap(),
            ),
            (
                Utc.with_ymd_and_hms(2026, 7, 8, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 7, 14, 23, 59, 59).unwrap(),
            ),
        ];
        let mut run_ids = Vec::new();
        for (start, end) in periods {
            let run = db::marketplace::create_manual_run(
                &pool,
                connection.id,
                &db::marketplace::CreateAmazonReportRunInput {
                    marketplace_id: "A1PA6795UKMFR9".to_owned(),
                    report_type: marketplace::SALES_AND_TRAFFIC.to_owned(),
                    data_start_time: Some(start),
                    data_end_time: Some(end),
                    report_options: json!({}),
                },
            )
            .await
            .unwrap();
            run_ids.push(run.id);
            for _ in 0..5 {
                worker.cycle(&pool).await.unwrap();
            }
        }
        let second = db::marketplace::get_run_detail(&pool, run_ids[1])
            .await
            .unwrap()
            .unwrap();
        let changes = second.analyses[0].result["changes_since_last_run"]
            .as_array()
            .unwrap();
        assert!(!changes.is_empty());
        assert!(changes.iter().all(|change| change["evidence_refs"]
            .as_array()
            .is_some_and(|refs| refs.len() == 2)));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn compressed_download_keeps_transport_bytes_and_decoded_parser_bytes(
        pool: sqlx::PgPool,
    ) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let connection = db::marketplace::upsert_connection(
            &pool,
            &db::marketplace::AmazonConnectionInput {
                seller_id: "DEMO-GZIP".to_owned(),
                region: "eu".to_owned(),
                secret_ref: "fixture:gzip".to_owned(),
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
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient));
        for _ in 0..5 {
            worker.cycle(&pool).await.unwrap();
        }
        let (raw, decoded, raw_hash, decoded_hash): (Vec<u8>, Vec<u8>, String, String) =
            sqlx::query_as(
                "SELECT raw_content, decoded_content, sha256, decoded_sha256
                 FROM amazon_report_documents WHERE run_id = $1",
            )
            .bind(run.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(&raw[..2], &[0x1f, 0x8b]);
        assert_eq!(decoded, SALES_FIXTURE.as_bytes());
        assert_eq!(raw_hash, sha256(&raw));
        assert_eq!(decoded_hash, sha256(&decoded));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn unknown_fixture_report_is_raw_archived_but_never_marked_analysed(pool: sqlx::PgPool) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let connection = db::marketplace::create_demo_connection(&pool)
            .await
            .unwrap();
        let run = db::marketplace::create_manual_run(
            &pool,
            connection.id,
            &db::marketplace::CreateAmazonReportRunInput {
                marketplace_id: "A1PA6795UKMFR9".to_owned(),
                report_type: "GET_SYNTHETIC_UNKNOWN_REPORT".to_owned(),
                data_start_time: None,
                data_end_time: None,
                report_options: json!({}),
            },
        )
        .await
        .unwrap();
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient));
        for _ in 0..4 {
            worker.cycle(&pool).await.unwrap();
        }
        let detail = db::marketplace::get_run_detail(&pool, run.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.run.status, "archived");
        assert_eq!(detail.document.unwrap().import_status, "unsupported");
        assert!(detail.snapshot.is_none());
        assert!(detail.analyses.is_empty());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn disabled_module_stops_due_jobs_and_late_schedule_runs_once_after_enable(
        pool: sqlx::PgPool,
    ) {
        let connection = db::marketplace::create_demo_connection(&pool)
            .await
            .unwrap();
        let schedule = db::marketplace::upsert_schedule(
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
        sqlx::query("UPDATE amazon_report_schedules SET next_run_at = now() - interval '1 hour' WHERE id = $1")
            .bind(schedule.id)
            .execute(&pool)
            .await
            .unwrap();
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient));
        worker.cycle(&pool).await.unwrap();
        let disabled_count: i64 = sqlx::query_scalar("SELECT count(*) FROM amazon_report_runs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(disabled_count, 0);
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        worker.cycle(&pool).await.unwrap();
        worker.cycle(&pool).await.unwrap();
        let enabled_count: i64 = sqlx::query_scalar("SELECT count(*) FROM amazon_report_runs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(enabled_count, 1);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn fixture_terminal_and_retry_states_preserve_raw_data(pool: sqlx::PgPool) {
        db::modules::set_enabled(&pool, db::modules::MARKETPLACE_INTELLIGENCE, true)
            .await
            .unwrap();
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient));
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
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient));
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
        let worker = MarketplaceWorker::new(Arc::new(FixtureAmazonClient));
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
