use crate::docker::{BollardRuntime, ContainerRuntime, CreateContainerOpts};
use crate::error::{Result, SpawnError};
use crate::output::Output;
use bollard::container::{
    Config, CreateContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures::StreamExt;
use std::collections::HashMap;
use std::io::Write as _;
use tokio::io::AsyncWriteExt;

impl ContainerRuntime for BollardRuntime {
    async fn ensure_running(&self) -> Result<()> {
        self.client()
            .ping()
            .await
            .map_err(|_| SpawnError::DockerNotRunning)?;
        Ok(())
    }

    async fn pull_image(&self, image: &str, output: &Output) -> Result<()> {
        self.pull_image_impl(image, output).await
    }

    async fn create_container(&self, opts: CreateContainerOpts) -> Result<String> {
        let mut port_bindings = HashMap::new();
        let mut exposed_ports = HashMap::new();

        for (host_port, container_port) in &opts.port_bindings {
            let container_port_key = format!("{container_port}/tcp");
            exposed_ports.insert(container_port_key.clone(), HashMap::new());
            port_bindings.insert(
                container_port_key,
                Some(vec![bollard::service::PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }

        let binds: Vec<String> = opts
            .bind_mounts
            .iter()
            .map(|(host, container)| format!("{host}:{container}"))
            .collect();

        let host_config = bollard::service::HostConfig {
            port_bindings: Some(port_bindings),
            binds: Some(binds),
            ..Default::default()
        };

        let config: Config<String> = Config {
            image: Some(opts.image.clone()),
            working_dir: Some(opts.working_dir),
            exposed_ports: Some(exposed_ports),
            env: Some(opts.env),
            host_config: Some(host_config),
            tty: Some(true),
            open_stdin: Some(true),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };

        let create_opts = CreateContainerOptions {
            name: &opts.name,
            platform: None,
        };

        let response = self
            .client()
            .create_container(Some(create_opts), config)
            .await
            .map_err(|e| SpawnError::ContainerCreate {
                reason: e.to_string(),
            })?;

        Ok(response.id)
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        self.client()
            .start_container(id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| SpawnError::ContainerStart {
                reason: e.to_string(),
            })?;
        Ok(())
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        self.client()
            .stop_container(id, Some(StopContainerOptions { t: 10 }))
            .await?;
        Ok(())
    }

    async fn is_container_running(&self, id: &str) -> Result<bool> {
        match self.client().inspect_container(id, None).await {
            Ok(info) => {
                let running = info.state.and_then(|s| s.running).unwrap_or(false);
                Ok(running)
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn exec_in_container(&self, id: &str, cmd: Vec<&str>, output: &Output) -> Result<i64> {
        let exec = self
            .client()
            .create_exec(
                id,
                CreateExecOptions {
                    cmd: Some(cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        let start_result = self.client().start_exec(&exec.id, None).await?;

        if let StartExecResults::Attached {
            output: mut stream, ..
        } = start_result
        {
            while let Some(Ok(msg)) = stream.next().await {
                let text = format!("{msg}");
                for line in text.lines() {
                    output.stream_line(line);
                }
            }
        }

        // Get exit code
        let inspect = self.client().inspect_exec(&exec.id).await?;
        let exit_code = inspect.exit_code.unwrap_or(0);
        if exit_code != 0 {
            return Err(SpawnError::ExecFailed { code: exit_code });
        }
        Ok(exit_code)
    }

    async fn exec_interactive(&self, id: &str, cmd: Vec<&str>) -> Result<i64> {
        let exec = self
            .client()
            .create_exec(
                id,
                CreateExecOptions {
                    cmd: Some(cmd),
                    attach_stdin: Some(true),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    tty: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        let start_result = self.client().start_exec(&exec.id, None).await?;

        if let StartExecResults::Attached {
            output: mut stream,
            mut input,
            ..
        } = start_result
        {
            // Spawn a task to forward stdin to the container
            tokio::spawn(async move {
                let mut stdin = tokio::io::stdin();
                let mut buf = [0u8; 1024];
                loop {
                    use tokio::io::AsyncReadExt;
                    match stdin.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if input.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // Forward container output to stdout
            let mut stdout = std::io::stdout();
            while let Some(Ok(msg)) = stream.next().await {
                let text = format!("{msg}");
                let _ = stdout.write_all(text.as_bytes());
                let _ = stdout.flush();
            }
        }

        // Get exit code
        let inspect = self.client().inspect_exec(&exec.id).await?;
        let exit_code = inspect.exit_code.unwrap_or(0);
        Ok(exit_code)
    }

    async fn copy_to_container(&self, id: &str, path: &str, data: &[u8]) -> Result<()> {
        use bollard::container::UploadToContainerOptions;
        use bytes::Bytes;

        let opts = UploadToContainerOptions {
            path,
            ..Default::default()
        };

        self.client()
            .upload_to_container(id, Some(opts), Bytes::from(data.to_vec()))
            .await?;

        Ok(())
    }
}
