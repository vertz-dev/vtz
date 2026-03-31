use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const POSTCSS_CONFIG_FILES: &[&str] = &[
    "postcss.config.js",
    "postcss.config.cjs",
    "postcss.config.mjs",
    "postcss.config.ts",
];

const POSTCSS_RUNNER: &str = r#"
import fs from 'node:fs/promises';
import path from 'node:path';
import Module, { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

function serializeError(error) {
  return {
    message: error?.reason ?? error?.message ?? String(error),
    line: typeof error?.line === 'number' ? error.line : null,
    column: typeof error?.column === 'number' ? error.column : null,
    file: error?.file ?? error?.input?.file ?? null,
  };
}

function normalizePlugins(plugins, requireFromConfig) {
  if (Array.isArray(plugins)) {
    return plugins;
  }

  if (!plugins || typeof plugins !== 'object') {
    return [];
  }

  return Object.entries(plugins)
    .filter(([, options]) => options !== false)
    .map(([name, options]) => {
      const loaded = requireFromConfig(name);
      const plugin = loaded?.default ?? loaded;
      if (typeof plugin !== 'function') {
        return plugin;
      }
      if (options === true || options == null) {
        return plugin();
      }
      return plugin(options);
    });
}

async function loadTsConfig(configPath, requireFromConfig) {
  const tsMod = requireFromConfig('typescript');
  const ts = tsMod?.default ?? tsMod;
  const source = await fs.readFile(configPath, 'utf8');
  const transpiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2020,
      esModuleInterop: true,
      allowSyntheticDefaultImports: true,
    },
    fileName: configPath,
  });

  const mod = new Module(configPath);
  mod.filename = configPath;
  mod.paths = Module._nodeModulePaths(path.dirname(configPath));
  mod.require = requireFromConfig;
  mod._compile(transpiled.outputText, configPath);
  return mod.exports;
}

async function loadConfig(configPath) {
  const requireFromConfig = createRequire(configPath);
  let loaded;

  if (configPath.endsWith('.ts')) {
    loaded = await loadTsConfig(configPath, requireFromConfig);
  } else {
    loaded = await import(`${pathToFileURL(configPath).href}?t=${Date.now()}`);
  }

  return {
    config: loaded?.default ?? loaded,
    requireFromConfig,
  };
}

async function main() {
  const rootDir = process.env.VTZ_ROOT_DIR;
  const cssPath = process.env.VTZ_CSS_PATH;
  const configPath = process.env.VTZ_POSTCSS_CONFIG;

  if (!rootDir || !cssPath || !configPath) {
    throw new Error('Missing PostCSS runner environment');
  }

  const postcssMod = createRequire(path.join(rootDir, 'package.json'))('postcss');
  const postcss = postcssMod?.default ?? postcssMod;
  const css = await fs.readFile(cssPath, 'utf8');

  const { config: loadedConfig, requireFromConfig } = await loadConfig(configPath);
  const ctx = {
    env: process.env.NODE_ENV ?? 'development',
    cwd: rootDir,
    file: {
      dirname: path.dirname(cssPath),
      basename: path.basename(cssPath),
      extname: path.extname(cssPath),
    },
    options: {},
  };

  let config = loadedConfig;
  if (typeof config === 'function') {
    config = await config(ctx);
  }
  if (!config || typeof config !== 'object') {
    config = {};
  }

  const { plugins: rawPlugins = [], ...processOptions } = config;
  const plugins = normalizePlugins(rawPlugins, requireFromConfig);
  const result = await postcss(plugins).process(css, {
    ...processOptions,
    from: cssPath,
    map: false,
  });

  process.stdout.write(JSON.stringify({ css: result.css }));
}

main().catch((error) => {
  process.stdout.write(JSON.stringify({ error: serializeError(error) }));
  process.exit(1);
});
"#;

#[derive(Debug, Clone)]
pub struct PostCssError {
    pub message: String,
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
struct RunnerOutput {
    css: Option<String>,
    error: Option<RunnerError>,
}

#[derive(Debug, serde::Deserialize)]
struct RunnerError {
    message: String,
    file: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
}

pub fn find_postcss_config(root_dir: &Path) -> Option<PathBuf> {
    POSTCSS_CONFIG_FILES
        .iter()
        .map(|name| root_dir.join(name))
        .find(|path| path.is_file())
}

pub fn process_css(
    root_dir: &Path,
    file_path: &Path,
    config_path: &Path,
) -> Result<String, PostCssError> {
    let node = which::which("node").map_err(|err| PostCssError {
        message: format!("PostCSS requires Node.js in PATH: {}", err),
        file: Some(file_path.to_path_buf()),
        line: None,
        column: None,
    })?;

    let mut child = Command::new(node)
        .arg("--input-type=module")
        .arg("-")
        .current_dir(root_dir)
        .env("VTZ_ROOT_DIR", root_dir)
        .env("VTZ_CSS_PATH", file_path)
        .env("VTZ_POSTCSS_CONFIG", config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| PostCssError {
            message: format!("Failed to start PostCSS: {}", err),
            file: Some(file_path.to_path_buf()),
            line: None,
            column: None,
        })?;

    // Write the runner script then close stdin to signal EOF.
    // If writing fails, kill the child to avoid a leaked process.
    {
        let mut stdin = child.stdin.take().expect("stdin was set to piped");
        if let Err(err) = stdin.write_all(POSTCSS_RUNNER.as_bytes()) {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            return Err(PostCssError {
                message: format!("Failed to send PostCSS runner to Node.js: {}", err),
                file: Some(file_path.to_path_buf()),
                line: None,
                column: None,
            });
        }
    } // stdin dropped here — pipe closed

    let output = child.wait_with_output().map_err(|err| PostCssError {
        message: format!("Failed to wait for PostCSS: {}", err),
        file: Some(file_path.to_path_buf()),
        line: None,
        column: None,
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let parsed = serde_json::from_str::<RunnerOutput>(&stdout).ok();

    if output.status.success() {
        if let Some(parsed) = parsed {
            if let Some(css) = parsed.css {
                return Ok(css);
            }
        }

        return Err(PostCssError {
            message: if stderr.is_empty() {
                "PostCSS returned an invalid response".to_string()
            } else {
                format!("PostCSS returned an invalid response: {}", stderr)
            },
            file: Some(file_path.to_path_buf()),
            line: None,
            column: None,
        });
    }

    if let Some(parsed) = parsed {
        if let Some(error) = parsed.error {
            return Err(PostCssError {
                message: error.message,
                file: error
                    .file
                    .map(PathBuf::from)
                    .or_else(|| Some(file_path.to_path_buf())),
                line: error.line,
                column: error.column,
            });
        }
    }

    let message = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    Err(PostCssError {
        message: if message.is_empty() {
            "PostCSS failed".to_string()
        } else {
            message
        },
        file: Some(file_path.to_path_buf()),
        line: None,
        column: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_postcss_config_prefers_js() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("postcss.config.js"), "module.exports = {};").unwrap();
        std::fs::write(
            tmp.path().join("postcss.config.cjs"),
            "module.exports = {};",
        )
        .unwrap();

        let config = find_postcss_config(tmp.path());
        assert_eq!(config, Some(tmp.path().join("postcss.config.js")));
    }

    #[test]
    fn test_find_postcss_config_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(find_postcss_config(tmp.path()), None);
    }
}
