use crate::docker::ContainerRuntime;
use crate::error::Result;
use include_dir::{include_dir, Dir};

/// Embedded Next.js template directory, compiled into the binary.
static NEXTJS_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/nextjs");

/// Write all embedded template files into a container, replacing placeholders.
pub async fn scaffold_into_container(
    runtime: &impl ContainerRuntime,
    container_id: &str,
    project_name: &str,
) -> Result<()> {
    let tar_data = build_tar_archive(project_name)?;
    runtime
        .copy_to_container(container_id, "/app", &tar_data)
        .await?;
    Ok(())
}

/// Build a tar archive from the embedded template files with placeholder replacement.
fn build_tar_archive(project_name: &str) -> Result<Vec<u8>> {
    let mut archive = tar::Builder::new(Vec::new());

    write_dir_to_tar(&mut archive, &NEXTJS_TEMPLATE, "", project_name)?;

    let data = archive.into_inner()?;
    Ok(data)
}

fn write_dir_to_tar(
    archive: &mut tar::Builder<Vec<u8>>,
    dir: &Dir,
    prefix: &str,
    project_name: &str,
) -> Result<()> {
    for file in dir.files() {
        let path = if prefix.is_empty() {
            file.path().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, file.path().to_string_lossy())
        };

        let contents = file.contents();
        // Try to replace placeholders in text files
        let processed = if is_text_file(&path) {
            let text = String::from_utf8_lossy(contents);
            let replaced = text.replace("{{PROJECT_NAME}}", project_name);
            replaced.into_bytes()
        } else {
            contents.to_vec()
        };

        let mut header = tar::Header::new_gnu();
        header.set_size(processed.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        archive.append_data(&mut header, &path, &*processed)?;
    }

    for subdir in dir.dirs() {
        let subdir_prefix = if prefix.is_empty() {
            subdir.path().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, subdir.path().to_string_lossy())
        };
        write_dir_to_tar(archive, subdir, &subdir_prefix, project_name)?;
    }

    Ok(())
}

fn is_text_file(path: &str) -> bool {
    let text_extensions = [
        ".json", ".ts", ".tsx", ".js", ".jsx", ".css", ".html", ".md", ".txt", ".yaml", ".yml",
        ".toml", ".env", ".template", ".config",
    ];
    text_extensions.iter().any(|ext| path.ends_with(ext))
}
