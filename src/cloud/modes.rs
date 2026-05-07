use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CostMode {
    Economy,
    Balanced,
    AlwaysOn,
}

impl fmt::Display for CostMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CostMode::Economy => write!(f, "economy"),
            CostMode::Balanced => write!(f, "balanced"),
            CostMode::AlwaysOn => write!(f, "always-on"),
        }
    }
}

impl FromStr for CostMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "economy" => Ok(Self::Economy),
            "balanced" => Ok(Self::Balanced),
            "always-on" | "always_on" => Ok(Self::AlwaysOn),
            other => Err(format!("cost mode '{other}' is not supported")),
        }
    }
}

impl CostMode {
    pub fn terraform_values(self) -> Value {
        match self {
            CostMode::Economy => json!({
                "mode": "economy",
                "enable_warm_pool": false,
                "asg_min_size": 0,
                "asg_desired_capacity": 0,
                "asg_max_size": 1,
                "use_spot_instances": true
            }),
            CostMode::Balanced => json!({
                "mode": "balanced",
                "enable_warm_pool": true,
                "asg_min_size": 0,
                "asg_desired_capacity": 0,
                "asg_max_size": 1,
                "use_spot_instances": false
            }),
            CostMode::AlwaysOn => json!({
                "mode": "always-on",
                "enable_warm_pool": false,
                "asg_min_size": 1,
                "asg_desired_capacity": 1,
                "asg_max_size": 1,
                "use_spot_instances": false
            }),
        }
    }

    pub fn rows() -> Vec<Value> {
        vec![
            json!({"mode":"economy","idle_cost_expectation":"lowest","start_expectation":"coldest path","terraform_behavior":"ASG min=0 desired=0 max=1, warm pool disabled, Spot instances"}),
            json!({"mode":"balanced","idle_cost_expectation":"some storage/EBS or warm-pool related cost","start_expectation":"warmer path when warm pool has capacity","terraform_behavior":"ASG min=0 desired=0 max=1, warm pool enabled, on-demand instances"}),
            json!({"mode":"always-on","idle_cost_expectation":"highest","start_expectation":"fastest response, no scale-to-zero idle state","terraform_behavior":"ASG min=1 desired=1 max=1, warm pool disabled, on-demand instances"}),
        ]
    }
}
