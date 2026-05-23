use crate::core::filters::{
  codename::{CODENAME_MAX_WORDS, codename},
  hash::short_hash,
  hash_port::hash_port,
  path_parts::{basename, dirname},
  sanitize::sanitize,
  sanitize_db::sanitize_db,
  sanitize_hash::sanitize_hash,
};
use crate::core::types::{CoreError, RenderContext};
use minijinja::{Environment, ErrorKind, UndefinedBehavior, value::Value};

/// Construct a minijinja environment with all custom filters registered.
fn build_env() -> Environment<'static> {
  let mut env = Environment::new();

  env.set_undefined_behavior(UndefinedBehavior::Strict);
  env.add_filter("hash_port", |v: String| -> u32 { hash_port(&v) as u32 });
  env.add_filter("sanitize", |v: String| -> String { sanitize(&v) });
  env.add_filter("sanitize_db", |v: String| -> String { sanitize_db(&v) });
  env.add_filter("sanitize_hash", |v: String| -> String { sanitize_hash(&v) });
  env.add_filter("hash", |v: String| -> String { short_hash(&v) });
  env.add_filter("dirname", |v: String| -> String { dirname(&v) });
  env.add_filter("basename", |v: String| -> String { basename(&v) });
  env.add_filter(
    "codename",
    |v: String, n: Option<u32>| -> Result<String, minijinja::Error> {
      let n = n.unwrap_or(2) as usize;

      if n == 0 || n > CODENAME_MAX_WORDS {
        return Err(minijinja::Error::new(
          ErrorKind::InvalidOperation,
          format!("codename word count must be between 1 and {CODENAME_MAX_WORDS}"),
        ));
      }

      Ok(codename(&v, n))
    },
  );

  env
}

/// Render a minijinja template string using the given context variables.
pub fn render(template: &str, ctx: &RenderContext) -> Result<String, CoreError> {
  let env = build_env();
  let tmpl = env
    .template_from_str(template)
    .map_err(|e| CoreError::TemplateRender(e.to_string()))?;
  let mut data = std::collections::BTreeMap::<String, Value>::new();

  data.insert("branch".into(), Value::from(ctx.branch.as_str()));

  if let Some(p) = ctx.worktree_path.as_ref() {
    data.insert("worktree_path".into(), Value::from(p.display().to_string()));
  }

  if let Some(n) = ctx.worktree_name.as_ref() {
    data.insert("worktree_name".into(), Value::from(n.as_str()));
  }

  if let Some(r) = ctx.repo.as_ref() {
    data.insert("repo".into(), Value::from(r.as_str()));
  }

  if let Some(p) = ctx.repo_path.as_ref() {
    data.insert("repo_path".into(), Value::from(p.display().to_string()));
  }

  if let Some(p) = ctx.cwd.as_ref() {
    data.insert("cwd".into(), Value::from(p.display().to_string()));
  }

  if let Some(t) = ctx.hook_type.as_ref() {
    data.insert("hook_type".into(), Value::from(t.as_str()));
  }

  if let Some(n) = ctx.hook_name.as_ref() {
    data.insert("hook_name".into(), Value::from(n.as_str()));
  }

  data.insert("args".into(), Value::from(ctx.args.clone()));

  for (k, v) in &ctx.vars {
    data.insert(k.clone(), Value::from(v.as_str()));
  }

  if !ctx.vars_state.is_empty() {
    let vars_obj: std::collections::BTreeMap<String, Value> = ctx
      .vars_state
      .iter()
      .map(|(k, v)| (k.clone(), Value::from(v.clone())))
      .collect();

    data.insert("vars".into(), Value::from(vars_obj));
  }

  tmpl
    .render(data)
    .map_err(|e| CoreError::TemplateRender(e.to_string()))
}
