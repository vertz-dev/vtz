use super::github::GitHubClient;
use super::output::PmOutput;
use super::tarball::extract_github_tarball;
use super::vertzrc::ScriptPolicy;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Errors specific to `vtz create` operations
#[derive(Debug)]
pub enum CreateError {
    /// Template string could not be parsed
    InvalidTemplate(String),
    /// Destination directory already exists and is not empty
    DestinationExists(PathBuf),
    /// GitHub API error
    GitHub(super::github::GitHubError),
    /// I/O or other error
    Other(String),
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateError::InvalidTemplate(t) => write!(f, "invalid template: \"{}\"", t),
            CreateError::DestinationExists(p) => {
                write!(
                    f,
                    "destination \"{}\" already exists and is not empty",
                    p.display()
                )
            }
            CreateError::GitHub(e) => write!(f, "{}", e),
            CreateError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for CreateError {}

impl From<super::github::GitHubError> for CreateError {
    fn from(e: super::github::GitHubError) -> Self {
        CreateError::GitHub(e)
    }
}

/// Parsed template identifying a GitHub repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRef {
    pub owner: String,
    pub repo: String,
}

/// Parse a template string into owner/repo.
///
/// Supported formats:
/// - Short name: `react` → `vertz-dev/create-react`
/// - Owner/repo: `vertz-dev/template-react`
/// - Full URL: `https://github.com/owner/repo`
pub fn parse_template(template: &str) -> Result<TemplateRef, CreateError> {
    let template = template.trim();

    if template.is_empty() {
        return Err(CreateError::InvalidTemplate(template.to_string()));
    }

    // Full URL: https://github.com/owner/repo
    if template.starts_with("https://github.com/") || template.starts_with("http://github.com/") {
        return parse_github_url(template);
    }

    // owner/repo format (contains exactly one slash, no protocol)
    if let Some((owner, repo)) = template.split_once('/') {
        if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') {
            return Ok(TemplateRef {
                owner: owner.to_string(),
                repo: repo.to_string(),
            });
        }
        return Err(CreateError::InvalidTemplate(template.to_string()));
    }

    // Short name: maps to vertz-dev/create-<name>
    Ok(TemplateRef {
        owner: "vertz-dev".to_string(),
        repo: format!("create-{}", template),
    })
}

fn parse_github_url(url: &str) -> Result<TemplateRef, CreateError> {
    // Strip protocol and host
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .unwrap_or("");

    // Strip trailing .git
    let path = path.strip_suffix(".git").unwrap_or(path);
    // Strip trailing slash
    let path = path.strip_suffix('/').unwrap_or(path);

    match path.split_once('/') {
        Some((owner, repo)) if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') => {
            Ok(TemplateRef {
                owner: owner.to_string(),
                repo: repo.to_string(),
            })
        }
        _ => Err(CreateError::InvalidTemplate(url.to_string())),
    }
}

/// Determine the destination directory.
///
/// If `dest` is provided, use it. Otherwise derive from the repo name
/// (stripping `create-` prefix if present).
pub fn resolve_destination(dest: Option<&str>, template_ref: &TemplateRef) -> PathBuf {
    if let Some(d) = dest {
        return PathBuf::from(d);
    }

    // For short-name templates (create-react), strip the "create-" prefix
    let name = template_ref
        .repo
        .strip_prefix("create-")
        .unwrap_or(&template_ref.repo);

    PathBuf::from(name)
}

/// Check that the destination is usable (doesn't exist or is empty).
pub fn validate_destination(dest: &Path) -> Result<(), CreateError> {
    if dest.exists() {
        // Allow if directory is empty
        if dest.is_dir() {
            let is_empty = std::fs::read_dir(dest)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if is_empty {
                return Ok(());
            }
        }
        return Err(CreateError::DestinationExists(dest.to_path_buf()));
    }
    Ok(())
}

/// Update the "name" field in package.json to match the project name.
pub fn update_package_name(
    dest: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pkg_path = dest.join("package.json");
    if !pkg_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&pkg_path)?;
    let mut pkg: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(obj) = pkg.as_object_mut() {
        obj.insert(
            "name".to_string(),
            serde_json::Value::String(name.to_string()),
        );
    }

    let output = serde_json::to_string_pretty(&pkg)?;
    std::fs::write(&pkg_path, format!("{}\n", output))?;
    Ok(())
}

/// Run `git init` and create an initial commit in the given directory.
fn git_init(dest: &Path) -> Result<(), CreateError> {
    let run = |args: &[&str]| -> Result<(), CreateError> {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dest)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| CreateError::Other(format!("failed to run git: {}", e)))?;
        if !status.success() {
            return Err(CreateError::Other(format!(
                "git {} failed with exit code {}",
                args.join(" "),
                status
            )));
        }
        Ok(())
    };

    run(&["init"])?;
    run(&["add", "-A"])?;
    run(&["commit", "-m", "Initial commit from vtz create"])?;
    Ok(())
}

/// Create a new project from a GitHub template.
///
/// Steps:
/// 1. Parse template → owner/repo
/// 2. Resolve destination directory
/// 3. Download & extract template tarball from GitHub
/// 4. Update package.json name
/// 5. Run `vtz install`
/// 6. `git init` + initial commit
pub async fn create(
    template: &str,
    dest: Option<&str>,
    output: Arc<dyn PmOutput>,
) -> Result<PathBuf, CreateError> {
    // 1. Parse template
    let template_ref = parse_template(template)?;
    output.info(&format!(
        "Using template {}/{}",
        template_ref.owner, template_ref.repo
    ));

    // 2. Resolve & validate destination
    let dest_dir = resolve_destination(dest, &template_ref);
    let dest_dir = if dest_dir.is_relative() {
        std::env::current_dir()
            .map_err(|e| CreateError::Other(format!("failed to get current dir: {}", e)))?
            .join(&dest_dir)
    } else {
        dest_dir
    };
    validate_destination(&dest_dir)?;

    let project_name = dest_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-app".to_string());

    // 3. Resolve latest commit and download tarball
    output.info("Resolving template...");
    let github = GitHubClient::new();
    let sha = github
        .resolve_ref(&template_ref.owner, &template_ref.repo, None)
        .await?;

    let tarball_url = GitHubClient::tarball_url(&template_ref.owner, &template_ref.repo, &sha);

    output.info("Downloading template...");
    let client = reqwest::Client::builder()
        .user_agent("vtz")
        .build()
        .map_err(|e| CreateError::Other(format!("failed to create HTTP client: {}", e)))?;

    let response = client
        .get(&tarball_url)
        .send()
        .await
        .map_err(|e| CreateError::Other(format!("failed to download template: {}", e)))?;

    if !response.status().is_success() {
        return Err(CreateError::Other(format!(
            "failed to download template: HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| CreateError::Other(format!("failed to read template tarball: {}", e)))?;

    // 4. Extract to destination
    output.info("Extracting template...");
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| CreateError::Other(format!("failed to create destination: {}", e)))?;

    let dest_clone = dest_dir.clone();
    let bytes_vec = bytes.to_vec();
    tokio::task::spawn_blocking(move || extract_github_tarball(&bytes_vec, &dest_clone))
        .await
        .map_err(|e| CreateError::Other(format!("extraction task failed: {}", e)))?
        .map_err(|e| CreateError::Other(format!("failed to extract template: {}", e)))?;

    // 5. Update package.json name
    update_package_name(&dest_dir, &project_name)
        .map_err(|e| CreateError::Other(format!("failed to update package.json: {}", e)))?;

    // 6. Install dependencies
    output.info("Installing dependencies...");
    if let Err(e) = super::install(
        &dest_dir,
        false,
        ScriptPolicy::TrustBased,
        false,
        output.clone(),
    )
    .await
    {
        output.warning(&format!(
            "install failed (you can run `vtz install` manually): {}",
            e
        ));
    }

    // 7. git init + initial commit
    output.info("Initializing git repository...");
    if let Err(e) = git_init(&dest_dir) {
        output.warning(&format!("git init failed: {}", e));
    }

    output.info(&format!(
        "Created {} at {}",
        project_name,
        dest_dir.display()
    ));
    Ok(dest_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_template tests ---

    #[test]
    fn test_parse_short_name() {
        let result = parse_template("react").unwrap();
        assert_eq!(
            result,
            TemplateRef {
                owner: "vertz-dev".to_string(),
                repo: "create-react".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_owner_repo() {
        let result = parse_template("vertz-dev/template-react").unwrap();
        assert_eq!(
            result,
            TemplateRef {
                owner: "vertz-dev".to_string(),
                repo: "template-react".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_full_url_https() {
        let result = parse_template("https://github.com/owner/my-template").unwrap();
        assert_eq!(
            result,
            TemplateRef {
                owner: "owner".to_string(),
                repo: "my-template".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_full_url_with_git_suffix() {
        let result = parse_template("https://github.com/owner/repo.git").unwrap();
        assert_eq!(
            result,
            TemplateRef {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_full_url_with_trailing_slash() {
        let result = parse_template("https://github.com/owner/repo/").unwrap();
        assert_eq!(
            result,
            TemplateRef {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_empty_string_errors() {
        assert!(parse_template("").is_err());
    }

    #[test]
    fn test_parse_whitespace_only_errors() {
        assert!(parse_template("   ").is_err());
    }

    #[test]
    fn test_parse_trims_whitespace() {
        let result = parse_template("  react  ").unwrap();
        assert_eq!(result.repo, "create-react");
    }

    #[test]
    fn test_parse_url_missing_repo_errors() {
        assert!(parse_template("https://github.com/owner").is_err());
    }

    #[test]
    fn test_parse_url_missing_owner_errors() {
        assert!(parse_template("https://github.com/").is_err());
    }

    // --- resolve_destination tests ---

    #[test]
    fn test_resolve_dest_explicit() {
        let tpl = TemplateRef {
            owner: "vertz-dev".to_string(),
            repo: "create-react".to_string(),
        };
        let dest = resolve_destination(Some("my-app"), &tpl);
        assert_eq!(dest, PathBuf::from("my-app"));
    }

    #[test]
    fn test_resolve_dest_from_short_name_template() {
        let tpl = TemplateRef {
            owner: "vertz-dev".to_string(),
            repo: "create-react".to_string(),
        };
        let dest = resolve_destination(None, &tpl);
        assert_eq!(dest, PathBuf::from("react"));
    }

    #[test]
    fn test_resolve_dest_from_owner_repo_template() {
        let tpl = TemplateRef {
            owner: "someone".to_string(),
            repo: "my-template".to_string(),
        };
        let dest = resolve_destination(None, &tpl);
        assert_eq!(dest, PathBuf::from("my-template"));
    }

    // --- validate_destination tests ---

    #[test]
    fn test_validate_dest_nonexistent_ok() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("new-project");
        assert!(validate_destination(&dest).is_ok());
    }

    #[test]
    fn test_validate_dest_empty_dir_ok() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("empty");
        std::fs::create_dir_all(&dest).unwrap();
        assert!(validate_destination(&dest).is_ok());
    }

    #[test]
    fn test_validate_dest_nonempty_dir_errors() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("nonempty");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("file.txt"), "content").unwrap();
        assert!(validate_destination(&dest).is_err());
    }

    #[test]
    fn test_validate_dest_existing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a-file");
        std::fs::write(&dest, "content").unwrap();
        assert!(validate_destination(&dest).is_err());
    }

    // --- update_package_name tests ---

    #[test]
    fn test_update_package_name_basic() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("package.json");
        std::fs::write(&pkg, r#"{"name": "template-name", "version": "0.1.0"}"#).unwrap();

        update_package_name(dir.path(), "my-app").unwrap();

        let content = std::fs::read_to_string(&pkg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["name"], "my-app");
        assert_eq!(parsed["version"], "0.1.0");
    }

    #[test]
    fn test_update_package_name_no_package_json_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        // No package.json — should be a no-op
        assert!(update_package_name(dir.path(), "my-app").is_ok());
    }

    #[test]
    fn test_update_package_name_preserves_fields() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("package.json");
        std::fs::write(
            &pkg,
            r#"{"name": "old", "version": "1.0.0", "dependencies": {"foo": "^1.0.0"}}"#,
        )
        .unwrap();

        update_package_name(dir.path(), "new-name").unwrap();

        let content = std::fs::read_to_string(&pkg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["name"], "new-name");
        assert_eq!(parsed["version"], "1.0.0");
        assert_eq!(parsed["dependencies"]["foo"], "^1.0.0");
    }
}
