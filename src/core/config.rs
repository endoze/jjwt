use crate::core::types::{Config, CoreError};

pub fn parse(src: &str) -> Result<Config, CoreError> {
    toml::from_str(src).map_err(|e| CoreError::ConfigParse(e.to_string()))
}
