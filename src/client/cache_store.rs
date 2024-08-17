use std::collections::HashMap;
use std::time::{Duration, Instant};


#[derive(Debug)]
struct CacheValue {
    value: String,
    expires_at: Option<Instant>,
}

#[derive(Debug)]
pub struct CacheStore {
    data: HashMap<String, CacheValue>,
}

impl CacheStore {
    pub fn new() -> Self {
        CacheStore {
            data: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: String, value: String, expiry: Option<Duration>) {
        let expires_at = match expiry {
            Some(expiry) => Some(Instant::now() + expiry),
            None => None,
        };
        self.data.insert(key, CacheValue { value, expires_at });
    }
    pub fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(|cache_value| {
            match cache_value.expires_at {
                Some(expiry) if expiry > Instant::now() => Some(cache_value.value.clone()),
                None => Some(cache_value.value.clone()),
                _ => None,
            }
        })
    }
}
