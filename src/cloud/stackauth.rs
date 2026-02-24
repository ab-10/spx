use anyhow::Result;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use crate::config::StackAuthConfig;
use crate::output;

/// Set up Stack Auth in the project container.
pub async fn setup(container_name: &str, project_dir: &Path) -> Result<StackAuthConfig> {
    // Run the Stack Auth installer in no-browser mode
    let status = if !container_name.is_empty() {
        Command::new("docker")
            .args([
                "exec",
                container_name,
                "npx",
                "@stackframe/init-stack",
                "--no-browser",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .await
    } else {
        Command::new("npx")
            .args(["@stackframe/init-stack", "--no-browser"])
            .current_dir(project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .await
    };

    match status {
        Ok(s) if s.success() => {
            output::success("Stack Auth configured.");
        }
        _ => {
            output::warn(
                "Stack Auth auto-setup did not complete. You may need to run \
                 `npx @stackframe/init-stack --no-browser` manually inside the container.",
            );
        }
    }

    // Generate a placeholder project ID — Stack Auth init would normally set this
    let project_id = uuid::Uuid::new_v4().to_string();

    Ok(StackAuthConfig { project_id })
}

/// Set up Stack Auth with placeholder env vars (for --local mode).
pub fn setup_local_placeholders(project_dir: &Path) -> Result<StackAuthConfig> {
    let env_path = project_dir.join(".env.local");
    let mut env_content = if env_path.exists() {
        std::fs::read_to_string(&env_path)?
    } else {
        String::new()
    };

    let stack_vars = [
        ("NEXT_PUBLIC_STACK_PROJECT_ID", "placeholder"),
        ("NEXT_PUBLIC_STACK_PUBLISHABLE_CLIENT_KEY", "placeholder"),
        ("STACK_SECRET_SERVER_KEY", "placeholder"),
    ];

    for (key, value) in &stack_vars {
        if !env_content.contains(key) {
            env_content.push_str(&format!("{key}={value}\n"));
        }
    }

    std::fs::write(&env_path, env_content)?;
    output::success("Stack Auth placeholder vars written to .env.local");

    Ok(StackAuthConfig {
        project_id: "placeholder".to_string(),
    })
}
