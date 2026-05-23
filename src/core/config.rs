use crate::core::types::{Config, CoreError, HookGroup};
use serde::Deserialize;

/// Parse a TOML string into a `Config`, mapping parse failures to `CoreError`.
pub fn parse(src: &str) -> Result<Config, CoreError> {
  toml::from_str(src).map_err(|e| CoreError::ConfigParse(e.to_string()))
}

/// Synthesized name used when a hook is defined as a bare string.
///
/// Worktrunk treats bare-string hooks as anonymous; jjwt's storage model
/// (`IndexMap<String, String>`) requires a key, so we pick a stable
/// placeholder so the same hook can be addressed by `jjwt hook default`
/// during introspection.
const DEFAULT_HOOK_NAME: &str = "default";

/// Accept three TOML shapes for a hook field and flatten them to the
/// internal `Vec<HookGroup>` representation:
///
/// * scalar string — `pre-start = "npm install"` becomes one group with a
///   single synthesized `default = "npm install"` entry.
/// * single table — `[pre-start]\nfoo = "..."` becomes one group with those
///   entries.
/// * array-of-tables — `[[pre-start]]\nfoo = "..."` keeps each block as its
///   own group, preserving pipeline ordering.
pub fn deserialize_hook_groups<'de, D>(d: D) -> Result<Vec<HookGroup>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  #[derive(Deserialize)]
  #[serde(untagged)]
  enum Repr {
    Scalar(String),
    Table(HookGroup),
    Pipeline(Vec<HookGroup>),
  }

  let repr = Option::<Repr>::deserialize(d)?;

  match repr {
    None => Ok(Vec::new()),
    Some(Repr::Scalar(cmd)) => {
      let mut g = HookGroup::new();

      g.insert(DEFAULT_HOOK_NAME.into(), cmd);

      Ok(vec![g])
    }
    Some(Repr::Table(g)) => {
      if g.is_empty() {
        Ok(Vec::new())
      } else {
        Ok(vec![g])
      }
    }
    Some(Repr::Pipeline(gs)) => Ok(gs),
  }
}
