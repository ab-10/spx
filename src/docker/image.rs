use crate::docker::BollardRuntime;
use crate::error::{Result, SpawnError};
use crate::output::Output;
use bollard::image::CreateImageOptions;
use futures::StreamExt;

impl BollardRuntime {
    /// Pull an image with streaming progress to the output layer.
    pub async fn pull_image_impl(&self, image: &str, output: &Output) -> Result<()> {
        let opts = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.client().create_image(Some(opts), None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    // Build a human-readable progress line
                    let status = info.status.unwrap_or_default();
                    let progress = info.progress.unwrap_or_default();
                    if !status.is_empty() {
                        if progress.is_empty() {
                            output.stream_line(&status);
                        } else {
                            output.stream_line(&format!("{status} {progress}"));
                        }
                    }
                }
                Err(e) => {
                    return Err(SpawnError::ImagePull {
                        image: image.to_string(),
                        reason: e.to_string(),
                    });
                }
            }
        }

        Ok(())
    }
}
