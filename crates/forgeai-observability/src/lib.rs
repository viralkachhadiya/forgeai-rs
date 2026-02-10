pub fn init_tracing() {
    let _ = tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::default());
}
