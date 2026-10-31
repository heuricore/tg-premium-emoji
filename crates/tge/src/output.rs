use serde_json::Value;

use crate::paths::Env;

#[derive(Debug, Clone)]
pub struct Ctx {
    pub json: bool,
    pub env: Env,
}

impl Ctx {
    /// Prints the JSON document in `--json` mode, the human text otherwise.
    pub fn emit(&self, human: &str, json: Value) {
        if self.json {
            println!("{json}");
        } else {
            println!("{human}");
        }
    }
}
