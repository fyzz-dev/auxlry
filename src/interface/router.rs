use anyhow::{Context, Result};
use minijinja::Environment;

/// Manages prompt templates using MiniJinja.
pub struct PromptRouter {
    env: Environment<'static>,
}

impl PromptRouter {
    pub fn new(locale: &str) -> Result<Self> {
        let mut env = Environment::new();

        // Load built-in templates
        let template_dir = format!("prompts/{locale}");
        let base_path = std::env::current_dir()?.join(&template_dir);

        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)
                .with_context(|| format!("failed to read template dir: {}", base_path.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "j2") {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let content = std::fs::read_to_string(&path)?;
                    env.add_template_owned(name, content)
                        .with_context(|| format!("failed to load template: {}", path.display()))?;
                }
            }
        }

        // Add default fallback templates
        env.add_template_owned(
            "interface_default".to_string(),
            include_str!("../../prompts/en/interface/default.md.j2").to_string(),
        )?;
        env.add_template_owned(
            "synapse_default".to_string(),
            include_str!("../../prompts/en/synapse/default.md.j2").to_string(),
        )?;
        env.add_template_owned(
            "operator_default".to_string(),
            include_str!("../../prompts/en/operator/default.md.j2").to_string(),
        )?;

        Ok(Self { env })
    }

    /// Render a template with the given context variables.
    pub fn render(&self, template_name: &str, ctx: minijinja::Value) -> Result<String> {
        let tmpl = self
            .env
            .get_template(template_name)
            .with_context(|| format!("template not found: {template_name}"))?;
        tmpl.render(ctx).context("failed to render template")
    }
}
