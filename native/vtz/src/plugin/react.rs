use crate::plugin::{
    ClientScript, CompileContext, CompileDiagnostic, CompileOutput, FrameworkPlugin,
};

/// The React framework plugin.
///
/// Provides React-specific compilation (TypeScript stripping, JSX transform),
/// import resolution for `react` / `react-dom`, and React-compatible HTML shell.
pub struct ReactPlugin {
    /// Whether to use the automatic JSX runtime (React 17+).
    pub jsx_runtime: JsxRuntime,
}

/// Which JSX runtime to use for React compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsxRuntime {
    /// `import { jsx } from 'react/jsx-runtime'` (React 17+, default)
    Automatic,
    /// `React.createElement` (classic, requires `import React`)
    Classic,
}

impl Default for ReactPlugin {
    fn default() -> Self {
        Self {
            jsx_runtime: JsxRuntime::Automatic,
        }
    }
}

impl FrameworkPlugin for ReactPlugin {
    fn name(&self) -> &str {
        "react"
    }

    fn compile(&self, source: &str, ctx: &CompileContext) -> CompileOutput {
        compile_react(source, ctx, self.jsx_runtime)
    }

    // resolve_import: uses default (None → /@deps/ resolution).
    // React packages (react, react-dom, react/jsx-runtime) are all bare
    // specifiers resolved via the default /@deps/ route.

    fn hmr_client_scripts(&self) -> Vec<ClientScript> {
        // Phase 2: no HMR scripts yet (added in Phase 3)
        vec![]
    }

    fn root_element_id(&self) -> &str {
        "root"
    }

    fn watch_extensions(&self) -> Vec<String> {
        vec![
            "ts".into(),
            "tsx".into(),
            "js".into(),
            "jsx".into(),
            "css".into(),
        ]
    }

    fn restart_triggers(&self) -> Vec<String> {
        vec![
            "package.json".into(),
            "tsconfig.json".into(),
            "bun.lock".into(),
            "bun.lockb".into(),
            ".env".into(),
            ".env.local".into(),
        ]
    }

    fn supports_fast_refresh(&self) -> bool {
        false // Phase 3 will enable this
    }
}

/// Compile a source file using oxc's transformer pipeline for React.
///
/// Steps:
/// 1. Parse with oxc_parser
/// 2. Build semantic scoping with oxc_semantic
/// 3. Transform (TypeScript strip + React JSX) with oxc_transformer
/// 4. Generate code with oxc_codegen
fn compile_react(source: &str, ctx: &CompileContext, jsx_runtime: JsxRuntime) -> CompileOutput {
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use oxc_transformer::{JsxOptions, JsxRuntime as OxcJsxRuntime, TransformOptions, Transformer};

    let allocator = Allocator::default();

    // Detect source type from file extension
    let source_type = SourceType::from_path(ctx.file_path).unwrap_or_default();

    // 1. Parse
    let parser_ret = Parser::new(&allocator, source, source_type).parse();

    let mut diagnostics = Vec::new();
    for error in &parser_ret.errors {
        diagnostics.push(CompileDiagnostic {
            message: error.to_string(),
            line: None,
            column: None,
            is_warning: false,
        });
    }

    if parser_ret.panicked {
        let escaped_path =
            serde_json::to_string(&ctx.file_path.display().to_string()).unwrap_or_default();
        return CompileOutput {
            code: format!(
                "console.error(\"[react] Parse error in \" + {});",
                escaped_path
            ),
            css: None,
            source_map: None,
            diagnostics,
        };
    }

    let mut program = parser_ret.program;

    // 2. Build semantic (needed for transformer scoping)
    let semantic_ret = SemanticBuilder::new().build(&program);

    for error in &semantic_ret.errors {
        diagnostics.push(CompileDiagnostic {
            message: error.to_string(),
            line: None,
            column: None,
            is_warning: true,
        });
    }

    let scoping = semantic_ret.semantic.into_scoping();

    // 3. Configure React JSX transform
    let oxc_jsx_runtime = match jsx_runtime {
        JsxRuntime::Automatic => OxcJsxRuntime::Automatic,
        JsxRuntime::Classic => OxcJsxRuntime::Classic,
    };

    let transform_options = TransformOptions {
        jsx: JsxOptions {
            runtime: oxc_jsx_runtime,
            ..JsxOptions::default()
        },
        ..TransformOptions::default()
    };

    let transformer = Transformer::new(&allocator, ctx.file_path, &transform_options);
    let transform_ret = transformer.build_with_scoping(scoping, &mut program);

    for error in &transform_ret.errors {
        diagnostics.push(CompileDiagnostic {
            message: error.to_string(),
            line: None,
            column: None,
            is_warning: true,
        });
    }

    // 4. Generate code
    let codegen_ret = Codegen::new().build(&program);

    CompileOutput {
        code: codegen_ret.code,
        css: None,
        source_map: codegen_ret.map.map(|sm| sm.to_json_string()),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::CompileContext;
    use std::path::Path;

    fn make_plugin() -> ReactPlugin {
        ReactPlugin::default()
    }

    fn make_ctx(file_path: &str) -> (&Path, &Path, &Path) {
        let fp = Path::new(file_path);
        let root = Path::new("/project");
        let src = Path::new("/project/src");
        (fp, root, src)
    }

    #[test]
    fn test_name() {
        assert_eq!(make_plugin().name(), "react");
    }

    #[test]
    fn test_root_element_id() {
        assert_eq!(make_plugin().root_element_id(), "root");
    }

    #[test]
    fn test_watch_extensions_include_jsx() {
        let exts = make_plugin().watch_extensions();
        assert!(exts.contains(&"jsx".to_string()));
        assert!(exts.contains(&"tsx".to_string()));
        assert!(exts.contains(&"js".to_string()));
    }

    #[test]
    fn test_restart_triggers() {
        let triggers = make_plugin().restart_triggers();
        assert!(triggers.contains(&"package.json".to_string()));
        assert!(triggers.contains(&"tsconfig.json".to_string()));
    }

    #[test]
    fn test_no_hmr_scripts_in_phase_2() {
        assert!(make_plugin().hmr_client_scripts().is_empty());
    }

    #[test]
    fn test_compile_simple_ts() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/utils.ts");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let output = plugin.compile("export const x: number = 1;", &ctx);
        assert!(
            output.diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            output.diagnostics
        );
        // TypeScript type annotation should be stripped
        assert!(
            output.code.contains("const x = 1"),
            "TS should be stripped, got: {}",
            output.code
        );
        assert!(
            !output.code.contains(": number"),
            "Type annotation should be removed, got: {}",
            output.code
        );
    }

    #[test]
    fn test_compile_tsx_automatic_runtime() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/App.tsx");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let output = plugin.compile(
            "export default function App() { return <div>Hello</div>; }",
            &ctx,
        );
        let errors: Vec<_> = output
            .diagnostics
            .iter()
            .filter(|d| !d.is_warning)
            .collect();
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        // Automatic runtime should produce jsx() calls
        assert!(
            output.code.contains("jsx(") || output.code.contains("jsxs("),
            "Expected jsx() call in output, got: {}",
            output.code
        );
        // Should import from react/jsx-runtime
        assert!(
            output.code.contains("react/jsx-runtime"),
            "Expected react/jsx-runtime import, got: {}",
            output.code
        );
    }

    #[test]
    fn test_compile_tsx_classic_runtime() {
        let plugin = ReactPlugin {
            jsx_runtime: JsxRuntime::Classic,
        };
        let (fp, root, src) = make_ctx("/project/src/App.tsx");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let output = plugin.compile(
            "import React from 'react';\nexport default function App() { return <div>Hello</div>; }",
            &ctx,
        );
        let errors: Vec<_> = output
            .diagnostics
            .iter()
            .filter(|d| !d.is_warning)
            .collect();
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        // Classic runtime should produce React.createElement calls
        assert!(
            output.code.contains("React.createElement"),
            "Expected React.createElement in output, got: {}",
            output.code
        );
    }

    #[test]
    fn test_compile_tsx_with_props() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/Button.tsx");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let source = r#"
            interface ButtonProps {
                label: string;
                disabled?: boolean;
            }
            export function Button({ label, disabled }: ButtonProps) {
                return <button disabled={disabled}>{label}</button>;
            }
        "#;
        let output = plugin.compile(source, &ctx);
        let errors: Vec<_> = output
            .diagnostics
            .iter()
            .filter(|d| !d.is_warning)
            .collect();
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        // Interface should be stripped
        assert!(
            !output.code.contains("interface ButtonProps"),
            "Interface should be stripped, got: {}",
            output.code
        );
        // JSX should be transformed
        assert!(
            output.code.contains("jsx(") || output.code.contains("jsxs("),
            "Expected jsx() call, got: {}",
            output.code
        );
    }

    #[test]
    fn test_compile_tsx_fragment() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/List.tsx");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let output = plugin.compile(
            "export function List() { return <><div>A</div><div>B</div></>; }",
            &ctx,
        );
        let errors: Vec<_> = output
            .diagnostics
            .iter()
            .filter(|d| !d.is_warning)
            .collect();
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        // Fragment should be transformed
        assert!(
            output.code.contains("Fragment") || output.code.contains("jsxs("),
            "Expected Fragment in output, got: {}",
            output.code
        );
    }

    #[test]
    fn test_compile_plain_js() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/utils.js");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let output = plugin.compile("export const add = (a, b) => a + b;", &ctx);
        assert!(output.diagnostics.is_empty());
        assert!(output.code.contains("const add"));
    }

    #[test]
    fn test_compile_parse_error() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/bad.tsx");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let output = plugin.compile("export function { broken syntax }", &ctx);
        assert!(!output.diagnostics.is_empty(), "Should have parse errors");
    }

    #[test]
    fn test_compile_tsx_spread_attributes() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/Spread.tsx");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let output = plugin.compile(
            "export function Spread(props: any) { return <div {...props} />; }",
            &ctx,
        );
        let errors: Vec<_> = output
            .diagnostics
            .iter()
            .filter(|d| !d.is_warning)
            .collect();
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        assert!(
            output.code.contains("jsx(") || output.code.contains("jsxs("),
            "Expected jsx() call, got: {}",
            output.code
        );
    }

    #[test]
    fn test_compile_tsx_nested_jsx() {
        let plugin = make_plugin();
        let (fp, root, src) = make_ctx("/project/src/Nested.tsx");
        let ctx = CompileContext {
            file_path: fp,
            root_dir: root,
            src_dir: src,
            target: "dom",
        };
        let source = r#"
            export function Layout() {
                return (
                    <div>
                        <header><h1>Title</h1></header>
                        <main><p>Body</p></main>
                    </div>
                );
            }
        "#;
        let output = plugin.compile(source, &ctx);
        let errors: Vec<_> = output
            .diagnostics
            .iter()
            .filter(|d| !d.is_warning)
            .collect();
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        // Multi-child should use jsxs
        assert!(
            output.code.contains("jsxs("),
            "Expected jsxs() for multi-child, got: {}",
            output.code
        );
        // Nested elements should also have jsx calls
        assert!(
            output.code.contains("jsx("),
            "Expected nested jsx() calls, got: {}",
            output.code
        );
    }

    #[test]
    fn test_default_jsx_runtime_is_automatic() {
        let plugin = ReactPlugin::default();
        assert_eq!(plugin.jsx_runtime, JsxRuntime::Automatic);
    }
}
