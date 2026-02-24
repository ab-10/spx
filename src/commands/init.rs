use crate::config::{ContainerConfig, SpawnConfig};
use crate::docker::{BollardRuntime, ContainerRuntime, CreateContainerOpts};
use crate::error::{Result, SpawnError};
use crate::output::Output;
use crate::templates;

const BASE_IMAGE: &str = "spawn-base:latest";

pub async fn run(name: Option<String>, local: bool, output: &Output) -> Result<()> {
    if !local {
        output.warn("Full cloud-connected init is not yet implemented.");
        output.next_step("Use `spawn init <name> --local` for local-only setup.");
        return Ok(());
    }

    let project_name = name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "my-app".to_string())
    });

    let container_name = format!("spawn-{project_name}");

    // Connect to Docker
    let runtime = BollardRuntime::connect()?;
    runtime.ensure_running().await?;

    // Step 1/4: Pull base image
    output.step(1, 4, &format!("Pulling spawn base image ({BASE_IMAGE})..."));
    match runtime.pull_image(BASE_IMAGE, output).await {
        Ok(_) => {}
        Err(SpawnError::ImagePull { .. }) => {
            output.warn("Could not pull image — it may need to be built locally.");
            output.stream_line(&format!(
                "  Run: docker build -t {BASE_IMAGE} ."
            ));
        }
        Err(e) => return Err(e),
    }

    // Step 2/4: Create and start container
    output.step(2, 4, "Creating and starting container...");
    let project_dir = std::env::current_dir()?.to_string_lossy().to_string();

    let container_id = runtime
        .create_container(CreateContainerOpts {
            image: BASE_IMAGE.to_string(),
            name: container_name.clone(),
            working_dir: "/app".to_string(),
            port_bindings: vec![(3000, 3000)],
            bind_mounts: vec![(project_dir, "/app".to_string())],
            env: vec![format!("PROJECT_NAME={project_name}")],
        })
        .await?;

    runtime.start_container(&container_id).await?;
    output.success(&format!("Container {container_name} started"));

    // Step 3/4: Scaffold Next.js app
    output.step(3, 4, "Scaffolding Next.js app...");
    templates::scaffold_into_container(&runtime, &container_id, &project_name).await?;

    // Run npm install inside the container
    output.stream_line("Installing dependencies...");
    runtime
        .exec_in_container(&container_id, vec!["npm", "install"], output)
        .await?;
    output.success("Dependencies installed");

    // Step 4/4: Start dev server
    output.step(4, 4, "Starting dev server...");
    // Start dev server in the background (detached)
    runtime
        .exec_in_container(
            &container_id,
            vec!["sh", "-c", "nohup npm run dev > /tmp/dev.log 2>&1 &"],
            output,
        )
        .await?;

    // Write config
    let config = SpawnConfig {
        project_name: project_name.clone(),
        container: ContainerConfig {
            image: BASE_IMAGE.to_string(),
            container_id: container_id.clone(),
            container_name: container_name.clone(),
        },
        local_only: true,
        cloud: None,
    };
    config.save(None)?;
    output.success("spawn.config.json written");

    // Summary
    output.success(&format!("Project '{project_name}' initialized!"));
    output.link("Open app", "http://localhost:3000");
    output.next_step(&format!(
        "Run `spawn run claude` to start an interactive Claude Code session in the container."
    ));

    Ok(())
}
