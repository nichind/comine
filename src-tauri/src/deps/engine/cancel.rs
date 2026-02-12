use dashmap::DashMap;
use std::sync::LazyLock;
use tokio_util::sync::CancellationToken;

static TOKENS: LazyLock<DashMap<&'static str, CancellationToken>> = LazyLock::new(DashMap::new);

pub fn reset_token(dep: &'static str) -> CancellationToken {
    let token = CancellationToken::new();
    TOKENS.insert(dep, token.clone());
    token
}

pub fn cancel(dep: &str) -> bool {
    if let Some(entry) = TOKENS.get(dep) {
        entry.cancel();
        true
    } else {
        false
    }
}
