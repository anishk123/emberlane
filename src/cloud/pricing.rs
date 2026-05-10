use crate::{error::EmberlaneError, util};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::Duration,
};
use tokio::{process::Command, task::JoinSet};

const AWS_CLI_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyPricePoint {
    pub hourly_usd: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstancePriceRecord {
    pub region: String,
    pub location: String,
    pub instance_type: String,
    pub on_demand: Option<HourlyPricePoint>,
    pub spot_min: Option<HourlyPricePoint>,
    pub spot_median: Option<HourlyPricePoint>,
    pub spot_max: Option<HourlyPricePoint>,
    pub refreshed_at: String,
    pub expires_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingCache {
    pub region: String,
    pub generated_at: String,
    pub expires_at: String,
    pub source: String,
    pub records: BTreeMap<String, InstancePriceRecord>,
}

fn cache_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("emberlane")
        .join("aws-prices")
}

fn cache_path(region: &str) -> PathBuf {
    cache_dir().join(format!("{}.json", region.trim()))
}

fn region_location(region: &str) -> String {
    match region {
        "us-west-2" => "US West (Oregon)".to_string(),
        "us-west-1" => "US West (N. California)".to_string(),
        "us-east-1" => "US East (N. Virginia)".to_string(),
        "us-east-2" => "US East (Ohio)".to_string(),
        "eu-west-1" => "EU (Ireland)".to_string(),
        "eu-west-2" => "EU (London)".to_string(),
        "eu-central-1" => "EU (Frankfurt)".to_string(),
        "ap-southeast-1" => "Asia Pacific (Singapore)".to_string(),
        "ap-southeast-2" => "Asia Pacific (Sydney)".to_string(),
        "ap-northeast-1" => "Asia Pacific (Tokyo)".to_string(),
        "ap-northeast-2" => "Asia Pacific (Seoul)".to_string(),
        other => other.to_string(),
    }
}

fn build_on_demand_args(instance_type: &str, location: &str) -> Vec<String> {
    vec![
        "pricing".to_string(),
        "get-products".to_string(),
        "--service-code".to_string(),
        "AmazonEC2".to_string(),
        "--filters".to_string(),
        format!("Type=TERM_MATCH,Field=instanceType,Value={instance_type}"),
        format!("Type=TERM_MATCH,Field=location,Value={location}"),
        "Type=TERM_MATCH,Field=operatingSystem,Value=Linux".to_string(),
        "Type=TERM_MATCH,Field=tenancy,Value=Shared".to_string(),
        "Type=TERM_MATCH,Field=preInstalledSw,Value=NA".to_string(),
        "Type=TERM_MATCH,Field=capacitystatus,Value=Used".to_string(),
        "--max-results".to_string(),
        "5".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ]
}

fn parse_rfc3339(ts: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

fn now_plus(days: i64) -> (String, String) {
    let now = util::now();
    let expires = now + chrono::Duration::days(days);
    (now.to_rfc3339(), expires.to_rfc3339())
}

pub fn is_stale(cache: &PricingCache) -> bool {
    parse_rfc3339(&cache.expires_at)
        .map(|expires| util::now() > expires)
        .unwrap_or(true)
}

pub fn load_cache(region: &str) -> Result<Option<PricingCache>, EmberlaneError> {
    let path = cache_path(region);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let cache: PricingCache = serde_json::from_str(&text)
        .map_err(|err| EmberlaneError::Internal(format!("failed to parse pricing cache: {err}")))?;
    Ok(Some(cache))
}

fn save_cache(cache: &PricingCache) -> Result<PathBuf, EmberlaneError> {
    let path = cache_path(&cache.region);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(cache).unwrap())?;
    Ok(path)
}

async fn run_aws(
    profile: Option<&str>,
    region: &str,
    args: &[String],
) -> Result<(i32, String, String), EmberlaneError> {
    let mut cmd = Command::new("aws");
    if let Some(profile) = profile.filter(|p| !p.trim().is_empty()) {
        cmd.arg("--profile").arg(profile);
    }
    cmd.arg("--region").arg(region);
    cmd.args(args);
    cmd.env("AWS_RETRY_MODE", "adaptive");
    cmd.env("AWS_MAX_ATTEMPTS", "10");
    cmd.env("AWS_PAGER", "");
    let output = tokio::time::timeout(Duration::from_secs(AWS_CLI_TIMEOUT_SECS), cmd.output())
        .await
        .map_err(|_| {
            EmberlaneError::Internal(format!(
                "aws cli timed out after {AWS_CLI_TIMEOUT_SECS}s while running pricing query for region {region}"
            ))
        })?
        .map_err(|err| EmberlaneError::Internal(format!("failed to run aws cli: {err}")))?;
    Ok((
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

async fn fetch_on_demand_price(
    profile: Option<&str>,
    region: &str,
    instance_type: &str,
) -> Result<Option<HourlyPricePoint>, EmberlaneError> {
    let location = region_location(region);
    let args = build_on_demand_args(instance_type, &location);
    let (status, stdout, stderr) = run_aws(profile, "us-east-1", &args).await?;
    if status != 0 {
        return Err(EmberlaneError::Internal(format!(
            "aws pricing get-products failed: {}",
            stderr.trim()
        )));
    }
    let parsed: Value = serde_json::from_str(&stdout).map_err(|err| {
        EmberlaneError::Internal(format!("failed to parse pricing response: {err}"))
    })?;
    let price_list = parsed
        .get("PriceList")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in price_list {
        let product: Value =
            serde_json::from_str(item.as_str().unwrap_or_default()).map_err(|err| {
                EmberlaneError::Internal(format!("failed to parse pricing product: {err}"))
            })?;
        let attributes = product
            .get("product")
            .and_then(|v| v.get("attributes"))
            .and_then(Value::as_object);
        if let Some(attributes) = attributes {
            let product_instance = attributes.get("instanceType").and_then(Value::as_str);
            let product_location = attributes.get("location").and_then(Value::as_str);
            if product_instance != Some(instance_type)
                || product_location != Some(location.as_str())
            {
                continue;
            }
        }
        if let Some(price) = product
            .get("terms")
            .and_then(|v| v.get("OnDemand"))
            .and_then(Value::as_object)
            .and_then(|terms| terms.values().next())
            .and_then(|term| term.get("priceDimensions"))
            .and_then(Value::as_object)
            .and_then(|dims| dims.values().next())
            .and_then(|dim| dim.get("pricePerUnit"))
            .and_then(|v| v.get("USD"))
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<f64>().ok())
        {
            return Ok(Some(HourlyPricePoint {
                hourly_usd: price,
                source: "aws pricing get-products".to_string(),
            }));
        }
    }
    Ok(None)
}

async fn fetch_spot_prices(
    profile: Option<&str>,
    region: &str,
    instance_type: &str,
) -> Result<Option<(HourlyPricePoint, HourlyPricePoint, HourlyPricePoint)>, EmberlaneError> {
    let args = vec![
        "ec2".to_string(),
        "describe-spot-price-history".to_string(),
        "--instance-types".to_string(),
        instance_type.to_string(),
        "--product-descriptions".to_string(),
        "Linux/UNIX (Amazon VPC)".to_string(),
        "--max-items".to_string(),
        "100".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];
    let (status, stdout, stderr) = run_aws(profile, region, &args).await?;
    if status != 0 {
        return Err(EmberlaneError::Internal(format!(
            "aws ec2 describe-spot-price-history failed: {}",
            stderr.trim()
        )));
    }
    let parsed: Value = serde_json::from_str(&stdout).map_err(|err| {
        EmberlaneError::Internal(format!("failed to parse spot price history: {err}"))
    })?;
    let mut prices = parsed
        .get("SpotPriceHistory")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            row.get("SpotPrice")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<f64>().ok())
        })
        .collect::<Vec<_>>();
    if prices.is_empty() {
        return Ok(None);
    }
    prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let min = *prices.first().unwrap_or(&0.0);
    let max = *prices.last().unwrap_or(&0.0);
    let median = if prices.len() % 2 == 0 {
        let upper = prices[prices.len() / 2];
        let lower = prices[prices.len() / 2 - 1];
        (lower + upper) / 2.0
    } else {
        prices[prices.len() / 2]
    };
    Ok(Some((
        HourlyPricePoint {
            hourly_usd: min,
            source: "aws ec2 describe-spot-price-history".to_string(),
        },
        HourlyPricePoint {
            hourly_usd: median,
            source: "aws ec2 describe-spot-price-history".to_string(),
        },
        HourlyPricePoint {
            hourly_usd: max,
            source: "aws ec2 describe-spot-price-history".to_string(),
        },
    )))
}

fn log_pricing_issue(kind: &str, region: &str, instance_type: &str, err: &EmberlaneError) {
    eprintln!("pricing {kind} lookup failed for {instance_type} in {region}: {err}");
}

async fn refresh_instance_price(
    profile: Option<&str>,
    region: &str,
    instance: &str,
) -> (String, InstancePriceRecord) {
    let on_demand = match fetch_on_demand_price(profile, region, instance).await {
        Ok(value) => value,
        Err(err) => {
            log_pricing_issue("on-demand", region, instance, &err);
            None
        }
    };
    let spot = match fetch_spot_prices(profile, region, instance).await {
        Ok(value) => value,
        Err(err) => {
            log_pricing_issue("spot", region, instance, &err);
            None
        }
    };
    let (spot_min, spot_median, spot_max) = spot
        .map(|(min, median, max)| (Some(min), Some(median), Some(max)))
        .unwrap_or((None, None, None));
    (
        instance.to_string(),
        InstancePriceRecord {
            region: region.to_string(),
            location: region_location(region),
            instance_type: instance.to_string(),
            on_demand,
            spot_min,
            spot_median,
            spot_max,
            refreshed_at: util::now().to_rfc3339(),
            expires_at: (util::now() + chrono::Duration::days(7)).to_rfc3339(),
            source: "aws pricing api".to_string(),
        },
    )
}

fn cache_from_results(
    region: &str,
    records: BTreeMap<String, InstancePriceRecord>,
) -> PricingCache {
    let (generated_at, expires_at) = now_plus(7);
    PricingCache {
        region: region.to_string(),
        generated_at,
        expires_at,
        source: "aws pricing api".to_string(),
        records,
    }
}

pub async fn refresh_cache(
    region: &str,
    instances: &[String],
    profile: Option<&str>,
) -> Result<PricingCache, EmberlaneError> {
    let unique = instances
        .iter()
        .filter(|instance| !instance.trim().is_empty())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut records = BTreeMap::new();
    let total = unique.len();
    if total > 0 {
        eprintln!(
            "Refreshing pricing cache for region '{}' across {} instance type(s)...",
            region, total
        );
    }
    let region = region.to_string();
    let profile = profile.map(|value| value.to_string());
    let instances = unique.into_iter().collect::<Vec<_>>();
    let batch_size = 2usize;
    let mut saw_error = false;
    for (batch_index, batch) in instances.chunks(batch_size).enumerate() {
        eprintln!(
            "starting pricing batch {}/{}",
            batch_index + 1,
            instances.len().div_ceil(batch_size)
        );
        let mut jobs = JoinSet::new();
        for instance in batch.iter().map(|value| value.to_string()) {
            let region = region.clone();
            let profile = profile.clone();
            eprintln!("[{}/{}] pricing for {}", records.len() + 1, total, instance);
            jobs.spawn(async move {
                Ok::<(String, InstancePriceRecord), EmberlaneError>(
                    refresh_instance_price(profile.as_deref(), &region, &instance).await,
                )
            });
        }
        while let Some(result) = jobs.join_next().await {
            match result {
                Ok(Ok((instance, record))) => {
                    records.insert(instance, record);
                }
                Ok(Err(err)) => {
                    saw_error = true;
                    eprintln!("pricing refresh task failed: {err}");
                }
                Err(err) => {
                    saw_error = true;
                    eprintln!(
                        "{}",
                        EmberlaneError::Internal(format!("pricing refresh task failed: {err}"))
                    );
                }
            }
        }
        let partial_cache = cache_from_results(&region, records.clone());
        let _ = save_cache(&partial_cache)?;
    }
    let cache = cache_from_results(&region, records);
    let _ = save_cache(&cache)?;
    if cache.records.is_empty() && saw_error {
        return Err(EmberlaneError::Internal(
            "pricing refresh failed for all instance types".to_string(),
        ));
    }
    Ok(cache)
}

pub async fn load_or_refresh(
    region: &str,
    instances: &[String],
    profile: Option<&str>,
    offline: bool,
) -> Result<(Option<PricingCache>, Vec<String>), EmberlaneError> {
    let mut warnings = Vec::new();
    let existing = load_cache(region)?;
    if offline {
        if let Some(cache) = existing {
            if is_stale(&cache) {
                warnings.push(format!(
                    "pricing cache for region '{}' is stale; showing cached estimates in offline mode",
                    region
                ));
            }
            return Ok((Some(cache), warnings));
        }
        return Ok((None, warnings));
    }
    let should_refresh = existing.as_ref().map(is_stale).unwrap_or(true);
    if should_refresh {
        match refresh_cache(region, instances, profile).await {
            Ok(cache) => return Ok((Some(cache), warnings)),
            Err(err) => {
                warnings.push(format!(
                    "pricing refresh failed for region '{}': {}",
                    region, err
                ));
                if let Some(cache) = existing {
                    warnings.push(
                        "showing stale pricing cache instead of blocking the command".to_string(),
                    );
                    return Ok((Some(cache), warnings));
                }
                return Err(err);
            }
        }
    }
    Ok((existing, warnings))
}

pub fn estimate_hourly(record: &InstancePriceRecord, use_spot: bool) -> Option<&HourlyPricePoint> {
    if use_spot {
        record.spot_median.as_ref().or(record.spot_min.as_ref())
    } else {
        record.on_demand.as_ref()
    }
}

pub fn summary_for_cache(cache: Option<PricingCache>) -> Value {
    match cache {
        Some(cache) => json!({
            "ok": true,
            "region": cache.region,
            "generated_at": cache.generated_at,
            "expires_at": cache.expires_at,
            "stale": is_stale(&cache),
            "source": cache.source,
            "records": cache.records,
        }),
        None => json!({
            "ok": false,
            "message": "No pricing cache exists yet. Run `emberlane aws prices refresh` first."
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn cache_staleness_works() {
        let (generated_at, _) = now_plus(7);
        let fresh = PricingCache {
            region: "us-west-2".to_string(),
            generated_at: generated_at.clone(),
            expires_at: (util::now() + chrono::Duration::days(1)).to_rfc3339(),
            source: "test".to_string(),
            records: BTreeMap::new(),
        };
        let stale = PricingCache {
            region: "us-west-2".to_string(),
            generated_at,
            expires_at: (util::now() - chrono::Duration::days(1)).to_rfc3339(),
            source: "test".to_string(),
            records: BTreeMap::new(),
        };
        assert!(!is_stale(&fresh));
        assert!(is_stale(&stale));
    }

    #[test]
    fn on_demand_args_use_single_filter_block_and_region_location() {
        let args = build_on_demand_args("g6e.2xlarge", "US West (Oregon)");
        let filter_count = args
            .iter()
            .filter(|arg| arg.as_str() == "--filters")
            .count();
        assert_eq!(filter_count, 1);
        assert!(args.contains(&"Type=TERM_MATCH,Field=instanceType,Value=g6e.2xlarge".to_string()));
        assert!(args.contains(&"Type=TERM_MATCH,Field=location,Value=US West (Oregon)".to_string()));
    }

    #[test]
    fn offline_cache_loading_uses_local_file_only() {
        let guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp = TempDir::new().unwrap();
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp.path());
        let cache = PricingCache {
            region: "us-west-2".to_string(),
            generated_at: util::now().to_rfc3339(),
            expires_at: (util::now() + chrono::Duration::days(1)).to_rfc3339(),
            source: "test".to_string(),
            records: BTreeMap::new(),
        };
        let path = save_cache(&cache).unwrap();
        assert!(path.exists());
        let loaded = load_cache("us-west-2").unwrap();
        assert!(loaded.is_some());
        assert!(!is_stale(&loaded.unwrap()));
        if let Some(old_home) = old_home {
            std::env::set_var("HOME", old_home);
        } else {
            std::env::remove_var("HOME");
        }
        drop(guard);
    }
}
