use dashmap::DashMap;
use lazy_static::lazy_static;
use tokio_util::sync::CancellationToken;

lazy_static! {
    static ref TOKENS: DashMap<&'static str, CancellationToken> = DashMap::new();
}

pub fn reset_token(dep: &'static str) -> CancellationToken {
    let token = CancellationToken::new();
    TOKENS.insert(dep, token.clone());
    token
}

pub fn cancel(dep: &'static str) -> bool {
    if let Some(entry) = TOKENS.get(dep) {
        entry.cancel();
        true
    } else {
        false
    }
}
