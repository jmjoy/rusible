use tracing_forest::{
    ForestLayer,
};
use tracing_subscriber::{
    EnvFilter, Registry,
    layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

pub fn init_forest_logging(default_filter: &str) {
    Registry::default()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(default_filter)),
        )
        .with(ForestLayer::default())
        .init();
}
