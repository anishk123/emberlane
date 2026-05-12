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
            "economy" | "spot" => Ok(Self::Economy),
            "balanced" | "on-demand" | "on_demand" => Ok(Self::Balanced),
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
                "enable_idle_scale_down": true,
                "asg_min_size": 0,
                "asg_desired_capacity": 1,
                "asg_max_size": 1,
                "use_spot_instances": true,
                "desired_capacity_on_wake": 1,
                "desired_capacity_on_sleep": 0
            }),
            CostMode::Balanced => json!({
                "mode": "balanced",
                "enable_warm_pool": false,
                "enable_idle_scale_down": true,
                "asg_min_size": 0,
                "asg_desired_capacity": 1,
                "asg_max_size": 1,
                "use_spot_instances": false,
                "desired_capacity_on_wake": 1,
                "desired_capacity_on_sleep": 0
            }),
            CostMode::AlwaysOn => json!({
                "mode": "always-on",
                "enable_warm_pool": false,
                "enable_idle_scale_down": false,
                "asg_min_size": 1,
                "asg_desired_capacity": 1,
                "asg_max_size": 1,
                "use_spot_instances": false,
                "desired_capacity_on_wake": 1,
                "desired_capacity_on_sleep": 1
            }),
        }
    }

    pub fn rows() -> Vec<Value> {
        vec![
            json!({"mode":"balanced","idle_cost_expectation":"some storage/EBS cost while running","start_expectation":"starts ready, then scales down after idle","terraform_behavior":"ASG min=0 desired=1 max=1, warm pool disabled by default, on-demand instances, idle-scale-down enabled"}),
            json!({"mode":"economy","idle_cost_expectation":"lowest","start_expectation":"starts ready, then scales down after idle","terraform_behavior":"ASG min=0 desired=1 max=1, warm pool disabled, Spot instances, idle scale-down enabled"}),
            json!({"mode":"always-on","idle_cost_expectation":"highest","start_expectation":"fastest response, no idle scale-down","terraform_behavior":"ASG min=1 desired=1 max=1, warm pool disabled, on-demand instances, idle scale-down disabled"}),
        ]
    }
}
