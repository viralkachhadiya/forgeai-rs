use forgeai_core::AdapterInfo;

pub fn pick_first_healthy(adapters: &[AdapterInfo]) -> Option<&AdapterInfo> {
    adapters.first()
}
