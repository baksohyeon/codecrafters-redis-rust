use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct CacheStore {
    data: HashMap<String, (String, Option<Instant>)>,
}



impl CacheStore {
    pub fn new() -> Self {
        CacheStore {
            data: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: String, value: String, expiry: Option<Duration>) {
        let expires_at = expiry.map(|duration| Instant::now() + duration);
        self.data.insert(key, (value, expires_at));
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(|(value, expires_at)| {
            match expires_at {
                Some(expiry) if expiry > &Instant::now() => Some(value.clone()),
                None => Some(value.clone()),
                _ => None,
            }
        })
    }
}