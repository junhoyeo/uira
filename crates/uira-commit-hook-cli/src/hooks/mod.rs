mod executor;

pub use executor::HookExecutor;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnFail {
    #[default]
    Continue,
    Stop,
    Warn,
}
