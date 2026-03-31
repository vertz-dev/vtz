use std::collections::HashMap;

/// Boolean-valued built-in env vars that should be emitted as unquoted literals.
const BOOLEAN_BUILTINS: &[&str] = &["DEV", "PROD"];

/// Replace `import.meta.env.KEY` and `import.meta.env` in compiled JavaScript.
///
/// - `import.meta.env.KEY` → literal value (string-quoted or boolean)
/// - `import.meta.env` (whole object) → `Object.freeze({...})`
///
/// Does not replace inside string literals or single-line comments.
pub fn replace_import_meta_env(code: &str, env: &HashMap<String, String>) -> String {
    let pattern = "import.meta.env";
    let chars: Vec<char> = code.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(code.len());
    let mut i = 0;

    while i < len {
        // Skip single-line comments
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            result.push_str(&collect_chars(&chars, start, i));
            continue;
        }

        // Skip multi-line comments
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2; // skip */
            }
            result.push_str(&collect_chars(&chars, start, i));
            continue;
        }

        // Skip string literals (single, double, template)
        if chars[i] == '\'' || chars[i] == '"' || chars[i] == '`' {
            let quote = chars[i];
            result.push(chars[i]);
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' {
                    result.push(chars[i]);
                    i += 1;
                    if i < len {
                        result.push(chars[i]);
                        i += 1;
                    }
                    continue;
                }
                result.push(chars[i]);
                i += 1;
            }
            if i < len {
                result.push(chars[i]); // closing quote
                i += 1;
            }
            continue;
        }

        // Check for `import.meta.env`
        let pat_chars: Vec<char> = pattern.chars().collect();
        if i + pat_chars.len() <= len && matches_at(&chars, i, &pat_chars) {
            let after_pattern = i + pat_chars.len();

            // Check if followed by `.KEY`
            if after_pattern < len && chars[after_pattern] == '.' {
                let key_start = after_pattern + 1;
                let key_end = scan_identifier(&chars, key_start, len);
                if key_end > key_start {
                    let key: String = chars[key_start..key_end].iter().collect();
                    if let Some(value) = env.get(&key) {
                        result.push_str(&format_env_value(&key, value));
                        i = key_end;
                        continue;
                    }
                }
            }

            // Whole object access: `import.meta.env` not followed by `.KEY`
            result.push_str(&format_env_object(env));
            i = after_pattern;
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Format a single env value as a JS literal.
///
/// Boolean built-ins (DEV, PROD) are emitted as unquoted `true`/`false`.
/// All other values are emitted as JSON strings.
fn format_env_value(key: &str, value: &str) -> String {
    if BOOLEAN_BUILTINS.contains(&key) {
        value.to_string()
    } else {
        format!("\"{}\"", escape_js_string(value))
    }
}

/// Format the entire env map as `Object.freeze({...})`.
fn format_env_object(env: &HashMap<String, String>) -> String {
    let mut entries: Vec<String> = env
        .iter()
        .map(|(k, v)| {
            let formatted_value = format_env_value(k, v);
            format!("\"{}\":{}", k, formatted_value)
        })
        .collect();
    entries.sort(); // deterministic output
    format!("Object.freeze({{{}}})", entries.join(","))
}

/// Escape a string for JS string literal context.
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn collect_chars(chars: &[char], start: usize, end: usize) -> String {
    chars[start..end].iter().collect()
}

fn matches_at(chars: &[char], pos: usize, pattern: &[char]) -> bool {
    if pos + pattern.len() > chars.len() {
        return false;
    }
    for (j, pc) in pattern.iter().enumerate() {
        if chars[pos + j] != *pc {
            return false;
        }
    }
    true
}

/// Scan an identifier starting at `pos`. Returns the end position.
fn scan_identifier(chars: &[char], pos: usize, len: usize) -> usize {
    let mut end = pos;
    while end < len && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '$') {
        end += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_env() -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("MODE".to_string(), "development".to_string());
        env.insert("DEV".to_string(), "true".to_string());
        env.insert("PROD".to_string(), "false".to_string());
        env.insert("BASE_URL".to_string(), "/".to_string());
        env.insert(
            "VITE_API_URL".to_string(),
            "https://api.example.com".to_string(),
        );
        env
    }

    // ── Basic key replacement ──────────────────────────────

    #[test]
    fn test_replace_string_env_var() {
        let env = test_env();
        let code = "const url = import.meta.env.VITE_API_URL;";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, r#"const url = "https://api.example.com";"#);
    }

    #[test]
    fn test_replace_boolean_dev() {
        let env = test_env();
        let code = "if (import.meta.env.DEV) {}";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, "if (true) {}");
    }

    #[test]
    fn test_replace_boolean_prod() {
        let env = test_env();
        let code = "if (import.meta.env.PROD) {}";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, "if (false) {}");
    }

    #[test]
    fn test_replace_mode() {
        let env = test_env();
        let code = "const mode = import.meta.env.MODE;";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, r#"const mode = "development";"#);
    }

    #[test]
    fn test_replace_base_url() {
        let env = test_env();
        let code = "const base = import.meta.env.BASE_URL;";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, r#"const base = "/";"#);
    }

    // ── Unknown key passthrough ────────────────────────────

    #[test]
    fn test_unknown_key_becomes_undefined() {
        let env = test_env();
        let code = "const x = import.meta.env.UNKNOWN_VAR;";
        let result = replace_import_meta_env(code, &env);
        // When key is not in env, we leave the full expression as
        // an object property access on the frozen object
        assert!(result.contains("Object.freeze("));
        assert!(result.contains(".UNKNOWN_VAR"));
    }

    // ── Whole object access ────────────────────────────────

    #[test]
    fn test_replace_whole_object() {
        let mut env = HashMap::new();
        env.insert("DEV".to_string(), "true".to_string());
        env.insert("MODE".to_string(), "development".to_string());

        let code = "const env = import.meta.env;";
        let result = replace_import_meta_env(code, &env);

        assert!(result.starts_with("const env = Object.freeze("));
        assert!(result.contains("\"DEV\":true"));
        assert!(result.contains("\"MODE\":\"development\""));
    }

    // ── String literal protection ──────────────────────────

    #[test]
    fn test_no_replace_in_double_quoted_string() {
        let env = test_env();
        let code = r#"const s = "import.meta.env.DEV";"#;
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, code);
    }

    #[test]
    fn test_no_replace_in_single_quoted_string() {
        let env = test_env();
        let code = "const s = 'import.meta.env.DEV';";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, code);
    }

    #[test]
    fn test_no_replace_in_template_literal() {
        let env = test_env();
        let code = "const s = `import.meta.env.DEV`;";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, code);
    }

    // ── Comment protection ─────────────────────────────────

    #[test]
    fn test_no_replace_in_single_line_comment() {
        let env = test_env();
        let code = "// import.meta.env.DEV\nconst x = 1;";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, code);
    }

    #[test]
    fn test_no_replace_in_multi_line_comment() {
        let env = test_env();
        let code = "/* import.meta.env.DEV */\nconst x = 1;";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, code);
    }

    // ── Multiple replacements ──────────────────────────────

    #[test]
    fn test_multiple_replacements_in_one_file() {
        let env = test_env();
        let code = "const dev = import.meta.env.DEV;\nconst url = import.meta.env.VITE_API_URL;";
        let result = replace_import_meta_env(code, &env);
        assert!(result.contains("const dev = true;"));
        assert!(result.contains(r#"const url = "https://api.example.com";"#));
    }

    // ── Code without import.meta.env is unchanged ──────────

    #[test]
    fn test_no_import_meta_env_unchanged() {
        let env = test_env();
        let code = "const x = 1;\nfunction foo() { return x + 2; }";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, code);
    }

    // ── Edge cases ─────────────────────────────────────────

    #[test]
    fn test_replace_in_jsx_attribute() {
        let env = test_env();
        let code = r#"<img src={import.meta.env.VITE_API_URL + "/avatar"} />"#;
        let result = replace_import_meta_env(code, &env);
        assert_eq!(
            result,
            r#"<img src={"https://api.example.com" + "/avatar"} />"#
        );
    }

    #[test]
    fn test_replace_in_ternary() {
        let env = test_env();
        let code = r#"const x = import.meta.env.DEV ? "dev" : "prod";"#;
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, r#"const x = true ? "dev" : "prod";"#);
    }

    #[test]
    fn test_value_with_special_chars_escaped() {
        let mut env = HashMap::new();
        env.insert(
            "VITE_MSG".to_string(),
            "hello \"world\"\nnewline".to_string(),
        );

        let code = "const msg = import.meta.env.VITE_MSG;";
        let result = replace_import_meta_env(code, &env);
        assert_eq!(result, r#"const msg = "hello \"world\"\nnewline";"#);
    }

    #[test]
    fn test_empty_code() {
        let env = test_env();
        let result = replace_import_meta_env("", &env);
        assert_eq!(result, "");
    }

    #[test]
    fn test_destructuring_whole_object() {
        let mut env = HashMap::new();
        env.insert("DEV".to_string(), "true".to_string());

        let code = "const { DEV } = import.meta.env;";
        let result = replace_import_meta_env(code, &env);
        assert!(result.contains("Object.freeze("));
        assert!(result.contains("\"DEV\":true"));
    }
}
