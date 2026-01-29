mod agent;

#[allow(unused_imports)]
pub use agent::{ConfigAgent, ConfigAgentWorker, PollOutcome};

#[doc(hidden)]
pub mod test_exports {
    use super::agent;
    use crate::state::RuntimeStateHandle;
    use reqwest::Client;
    use std::path::PathBuf;
    use std::sync::Arc;

    pub fn config_agent_new_for_tests(
        relay_base: String,
        lkg_path: PathBuf,
        state: Arc<RuntimeStateHandle>,
        client: Client,
    ) -> agent::ConfigAgent {
        agent::ConfigAgent::new_for_tests(relay_base, lkg_path, state, client)
    }
}
