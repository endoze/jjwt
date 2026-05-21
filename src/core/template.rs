use crate::core::filters::{hash_port::hash_port, sanitize::sanitize};
use crate::core::types::{CoreError, RenderContext};
use minijinja::{value::Value, Environment, UndefinedBehavior};

fn build_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.add_filter("hash_port", |v: String| -> u32 { hash_port(&v) as u32 });
    env.add_filter("sanitize", |v: String| -> String { sanitize(&v) });

    env
}

pub fn render(template: &str, ctx: &RenderContext) -> Result<String, CoreError> {
    let env = build_env();
    let tmpl = env
        .template_from_str(template)
        .map_err(|e| CoreError::TemplateRender(e.to_string()))?;
    let mut data = std::collections::BTreeMap::new();
    data.insert("branch", Value::from(ctx.branch.clone()));

    tmpl.render(data)
        .map_err(|e| CoreError::TemplateRender(e.to_string()))
}
