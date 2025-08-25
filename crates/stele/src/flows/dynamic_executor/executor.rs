// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.

use super::{
    assembly::AssemblyGenerator, dependency::DependencyManager, function::DynamicFunction,
    hot_reload::HotReloadManager, import_export::ImportExportManager, metrics::PerformanceMetrics,
};
use crate::blocks::rules::BlockError;
use crate::codegen::{guard_and_rewrite, rust_clean, wat_sanitize};
use chrono::{DateTime, Utc};
#[cfg(feature = "dynamic-native")]
use libloading::Library;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
#[allow(unused_imports)]
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tempfile::tempdir;
#[cfg(feature = "dynamic-wasi")]
use wasmtime::Linker;

type VersionHistory = Arc<RwLock<HashMap<String, HashMap<String, DateTime<Utc>>>>>;

pub struct DynamicExecutor {
    functions: Arc<RwLock<HashMap<String, Vec<DynamicFunction>>>>,
    function_compositions: Arc<RwLock<HashMap<String, Vec<String>>>>,
    version_history: VersionHistory,
    hot_reload_manager: Arc<RwLock<HotReloadManager>>,
    assembly_generator: AssemblyGenerator,
    #[cfg(feature = "dynamic-native")]
    library_registry: Arc<RwLock<LibraryRegistry>>,
}

#[cfg(feature = "dynamic-native")]
struct LibraryRegistry {
    libs: Vec<(String, Library)>,
}

#[cfg(feature = "dynamic-native")]
impl LibraryRegistry {
    fn new() -> Self {
        Self { libs: Vec::new() }
    }
    fn add(&mut self, id: String, lib: Library) {
        self.libs.push((id, lib));
    }
    #[allow(dead_code)]
    fn unload(&mut self, id: &str) {
        self.libs.retain(|(i, _)| i != id);
    }
    #[allow(dead_code)]
    fn unload_all(&mut self) {
        self.libs.clear();
    }
}

pub enum DynamicSource<'a> {
    Wat {
        name: &'a str,
        export: &'a str,
        wat: &'a str,
    },
    #[cfg(feature = "dynamic-native")]
    RustExpression { name: &'a str, body: &'a str },
    #[cfg(feature = "dynamic-native")]
    RustFull { name: &'a str, source: &'a str },
    #[cfg(feature = "dynamic-wasi")]
    RustWasiExpression { name: &'a str, body: &'a str },
    #[cfg(feature = "dynamic-wasi")]
    RustWasiFull { name: &'a str, source: &'a str },
}

impl DynamicExecutor {
    #[cfg(any(feature = "dynamic-native", feature = "dynamic-wasi"))]
    fn apply_pow_disambiguation(original: &str) -> String {
        if !original.contains(".pow(") && !original.contains(".powi(") {
            return original.to_string();
        }
        let mut fixed = original.to_string();
        if fixed.contains("(1..=") && !fixed.contains("(1u64..=") {
            fixed = fixed.replace("(1..=", "(1u64..=");
        }

        for bound in ["10", "25", "50", "100", "150", "200", "256", "512", "1000"] {
            let pat = format!("..={bound})");
            let repl = format!("..={bound}u64)");
            if fixed.contains(&pat) && !fixed.contains(&repl) {
                fixed = fixed.replace(&pat, &repl);
            }
        }

        if fixed.contains("|x| x.pow(") && !fixed.contains("|x: u64| x.pow(") {
            fixed = fixed.replace("|x| x.pow(", "|x: u64| x.pow(");
        }

        if fixed.contains("-1.0_f64.powi(")
            || fixed.contains("(-1.0_f64).powi(")
            || fixed.contains("-1.0.powi(")
            || fixed.contains("(-1.0).powi(")
        {
            let mut out = String::with_capacity(fixed.len());
            for line in fixed.lines() {
                let mut replaced_line = line.to_string();
                if replaced_line.contains("powi(") && replaced_line.contains("-1.0") {
                    for base in [
                        "(-1.0_f64).powi(",
                        "(-1.0).powi(",
                        "-1.0_f64.powi(",
                        "-1.0.powi(",
                    ] {
                        if let Some(start) = replaced_line.find(base) {
                            let after = &replaced_line[start + base.len()..];
                            if let Some(end) = after.find(')') {
                                let inner = &after[..end];
                                let replacement =
                                    format!("if (({inner}) % 2)==0 {{1.0_f64}} else {{-1.0_f64}}");
                                let orig = &replaced_line[start..start + base.len() + end + 1];
                                replaced_line = replaced_line.replacen(orig, &replacement, 1);
                                break;
                            }
                        }
                    }
                }
                out.push_str(&replaced_line);
                out.push('\n');
            }
            fixed = out;
        }
        while fixed.contains("u64u64") {
            fixed = fixed.replace("u64u64", "u64");
        }
        fixed
    }

    pub fn register_dynamic_source(
        &self,
        src: DynamicSource,
    ) -> Result<DynamicFunction, BlockError> {
        match src {
            DynamicSource::Wat {
                name: _,
                export,
                wat,
            } => {
                let (cleaned, _metrics) = wat_sanitize::sanitize_wat_basic(wat);
                self.compile_function(&cleaned, export)
            }
            #[cfg(feature = "dynamic-native")]
            DynamicSource::RustExpression { name: _, body } => {
                let cleaned = rust_clean::wrap_body_as_compute(body);
                self.compile_rust_cdylib(&cleaned)
            }
            #[cfg(feature = "dynamic-native")]
            DynamicSource::RustFull { name: _, source } => {
                let original = source.to_string();

                let cleaned = rust_clean::clean_rust_source(source).ok_or_else(|| {
                    BlockError::ProcessingError("rust source cleaning failed".into())
                })?;
                let dyn_fn = self.compile_rust_cdylib(&cleaned.source)?;

                let mut dyn_fn = dyn_fn;
                dyn_fn.metadata.insert(
                    "original_source".into(),
                    serde_json::Value::String(original),
                );
                dyn_fn.metadata.insert(
                    "guard_transform".into(),
                    serde_json::json!({"notes":"native path (no guard_and_rewrite invoked)"}),
                );
                Ok(dyn_fn)
            }
            #[cfg(feature = "dynamic-wasi")]
            DynamicSource::RustWasiExpression { name: _, body } => {
                let cleaned = rust_clean::wrap_body_as_compute(body);
                self.compile_rust_wasi(&cleaned)
            }
            #[cfg(feature = "dynamic-wasi")]
            DynamicSource::RustWasiFull { name: _, source } => {
                let original = source.to_string();
                let guarded = guard_and_rewrite(source)
                    .map_err(|e| BlockError::ProcessingError(format!("guard: {e}")))?;
                let cleaned = rust_clean::clean_rust_source(&guarded.source).ok_or_else(|| {
                    BlockError::ProcessingError("rust source cleaning failed".into())
                })?;
                let adjusted = Self::apply_pow_disambiguation(&cleaned.source);
                let mut dyn_fn = self.compile_rust_wasi(&adjusted)?;

                tracing::info!(
                    orig_len = original.len(),
                    guarded_len = guarded.source.len(),
                    "registered dynamic wasi source"
                );
                dyn_fn.metadata.insert(
                    "original_source".into(),
                    serde_json::Value::String(original),
                );
                dyn_fn.metadata.insert(
                    "guarded_source".into(),
                    serde_json::Value::String(guarded.source.clone()),
                );
                dyn_fn.metadata.insert(
                    "guard_transform".into(),
                    serde_json::json!({"pow_eliminated": false, "notes": "guard-only validation; no rewrites"}),
                );
                Ok(dyn_fn)
            }
        }
    }

    #[cfg(feature = "dynamic-native")]
    fn compile_rust_cdylib(&self, _src: &str) -> Result<DynamicFunction, BlockError> {
        let src = _src;

        let tmp =
            tempdir().map_err(|e| BlockError::ProcessingError(format!("tempdir failed: {e}")))?;
        let base = tmp.path().to_path_buf();

        let unique_nanos = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| chrono::Utc::now().timestamp() * 1_000_000_000);
        let crate_name = format!("genlib_{unique_nanos}");
        let proj = base.join(&crate_name);

        let home = std::env::var("HOME")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned());
        let cache_base: std::path::PathBuf =
            [home.as_str(), ".cache", "stele_dyn"].iter().collect();
        let shared_target = cache_base.join("target");
        if let Err(e) = fs::create_dir_all(&shared_target) {
            eprintln!(
                "warn: could not create shared target dir ({e}); falling back to per-build target"
            );
        }
        fs::create_dir_all(proj.join("src"))
            .map_err(|e| BlockError::ProcessingError(format!("io: {e}")))?;

        if shared_target.exists() {
            let cargo_cfg_dir = proj.join(".cargo");
            if let Err(e) = fs::create_dir_all(&cargo_cfg_dir) {
                eprintln!("warn: could not create .cargo dir: {e}");
            } else {
                let cfg = format!("[build]\ntarget-dir = \"{}\"\n", shared_target.display());
                if let Err(e) = fs::write(cargo_cfg_dir.join("config.toml"), cfg) {
                    eprintln!("warn: write config.toml failed: {e}");
                }
            }
        }
        fs::write(
            proj.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{crate_name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nserde = {{ version = \"1\", features=[\"derive\"] }}\nserde_json = \"1\"\n"
            ),
        )
        .map_err(|e| BlockError::ProcessingError(format!("write cargo: {e}")))?;
        fs::write(proj.join("src").join("lib.rs"), src)
            .map_err(|e| BlockError::ProcessingError(format!("write lib: {e}")))?;

        let run_build = || -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), BlockError> {
            let out = Command::new("cargo")
                .arg("build")
                .arg("--release")
                .current_dir(&proj)
                .output()
                .map_err(|e| BlockError::ProcessingError(format!("cargo exec failed: {e}")))?;
            Ok((out.status, out.stdout, out.stderr))
        };
        let (status1, _stdout1, stderr1) = run_build()?;
        let mut final_stderr = stderr1.clone();
        let mut build_succeeded = status1.success();
        if !status1.success() {
            let stderr_txt = String::from_utf8_lossy(&stderr1);
            let has_error_marker = stderr_txt.contains("error:");

            let mut attempted_pow_fix = false;
            if (stderr_txt.contains("E0689") || stderr_txt.contains("ambiguous numeric type"))
                && src.contains(".pow(")
            {
                let fixed = Self::apply_pow_disambiguation(src);
                if fixed != src {
                    if let Err(e) = fs::write(proj.join("src").join("lib.rs"), &fixed) {
                        eprintln!("warn: failed to apply pow fix: {e}");
                    } else {
                        attempted_pow_fix = true;
                        let (st_fix, _so_fix, se_fix) = run_build()?;
                        final_stderr = se_fix.clone();
                        if st_fix.success() {
                            build_succeeded = true;
                        }
                    }
                }
            }
            if !has_error_marker {
                let (status2, _stdout2, stderr2) = run_build()?;
                final_stderr = stderr2.clone();
                if !status2.success() {
                    let stderr_snip = String::from_utf8_lossy(&final_stderr);
                    let last_lines: String = stderr_snip
                        .lines()
                        .rev()
                        .take(15)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join(" | ");
                    return Err(BlockError::ProcessingError(format!(
                        "cargo build failed (after retry): {last_lines}"
                    )));
                } else {
                    build_succeeded = true;
                }
            } else if !attempted_pow_fix {
                let stderr_snip = String::from_utf8_lossy(&final_stderr);
                let last_lines: String = stderr_snip
                    .lines()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join(" | ");
                return Err(BlockError::ProcessingError(format!(
                    "cargo build failed: {last_lines}"
                )));
            }
        }
        if !build_succeeded {
            let stderr_snip = String::from_utf8_lossy(&final_stderr);
            let last_lines: String = stderr_snip
                .lines()
                .rev()
                .take(25)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ");
            return Err(BlockError::ProcessingError(format!(
                "cargo build failed (unresolved): {last_lines}"
            )));
        }

        let mut candidates = Vec::new();
        let default_target_dir = proj.join("target").join("release");
        candidates.push(default_target_dir.clone());
        if shared_target.exists() {
            candidates.push(shared_target.join("release"));
        }
        let mut dylib_opt = None;
        for dir in candidates {
            if let Ok(read) = std::fs::read_dir(&dir) {
                for entry in read.flatten() {
                    let p = entry.path();
                    if let Some(n) = p.file_name().and_then(|s| s.to_str()) {
                        if n.contains(&crate_name)
                            && (n.ends_with(".dylib") || n.ends_with(".so") || n.ends_with(".dll"))
                        {
                            dylib_opt = Some(p.clone());
                            break;
                        }
                    }
                }
            }
            if dylib_opt.is_some() {
                break;
            }
        }
        let dylib = dylib_opt
            .ok_or_else(|| BlockError::ProcessingError("compiled library not found".into()))?;

        unsafe {
            let lib = Library::new(&dylib)
                .map_err(|e| BlockError::ProcessingError(format!("load dylib: {e}")))?;
            let func: libloading::Symbol<
                unsafe extern "C" fn(*const u8, usize, *mut u8, usize) -> i32,
            > = lib
                .get(b"compute")
                .map_err(|e| BlockError::ProcessingError(format!("load symbol: {e}")))?;
            let func_ptr: unsafe extern "C" fn(*const u8, usize, *mut u8, usize) -> i32 = *func;

            if let Ok(mut reg) = self.library_registry.write() {
                let nanos = chrono::Utc::now().timestamp_nanos_opt().expect("valid ts");
                let id = format!("genlib-{nanos}");
                reg.add(id, lib);
            }
            let closure =
                move |inputs: &[serde_json::Value]| -> Result<serde_json::Value, BlockError> {
                    let json = serde_json::to_vec(inputs)
                        .map_err(|e| BlockError::ProcessingError(format!("serde: {e}")))?;
                    let mut out = vec![0u8; 8192];
                    let code = func_ptr(json.as_ptr(), json.len(), out.as_mut_ptr(), out.len());
                    if code != 0 {
                        return Err(BlockError::ProcessingError(format!(
                            "dyn fn error code {code}"
                        )));
                    }
                    let nul = out.iter().position(|b| *b == 0).unwrap_or(out.len());
                    let slice = &out[..nul];
                    let v: serde_json::Value = serde_json::from_slice(slice)
                        .map_err(|e| BlockError::ProcessingError(format!("json parse: {e}")))?;
                    Ok(v)
                };
            let dyn_fn = super::function::DynamicFunction::new(
                Arc::new(closure),
                format!("v{}", chrono::Utc::now().timestamp()),
                src.to_string(),
            );

            Ok(dyn_fn)
        }
    }
}
impl DynamicExecutor {
    pub fn new() -> Result<Self, BlockError> {
        let hot_reload_manager = HotReloadManager::new()?;
        let assembly_generator = AssemblyGenerator::new()?;
        Ok(Self {
            functions: Arc::new(RwLock::new(HashMap::new())),
            function_compositions: Arc::new(RwLock::new(HashMap::new())),
            version_history: Arc::new(RwLock::new(HashMap::new())),
            hot_reload_manager: Arc::new(RwLock::new(hot_reload_manager)),
            assembly_generator,
            #[cfg(feature = "dynamic-native")]
            library_registry: Arc::new(RwLock::new(LibraryRegistry::new())),
        })
    }
    pub fn compile_function(
        &self,
        wat_code: &str,
        exported_fn_name: &str,
    ) -> Result<DynamicFunction, BlockError> {
        let wasm_bytes = wat::parse_str(wat_code)
            .map_err(|e| BlockError::ProcessingError(format!("Invalid WAT format: {e}")))?;
        self.assembly_generator
            .compile_function(&wasm_bytes, exported_fn_name, wat_code)
    }
    pub fn register_function(&self, name: String, function: DynamicFunction) {
        let mut version_history = self.version_history.write().unwrap();
        let version_entry = version_history.entry(name.clone()).or_default();
        version_entry.insert(function.version.clone(), function.created_at);
        let mut functions = self.functions.write().unwrap();
        let functions_entry = functions.entry(name).or_default();
        functions_entry.push(function);
    }
    pub fn get_function(&self, name: &str, version: Option<&str>) -> Option<DynamicFunction> {
        let functions = self.functions.read().unwrap();
        let function_list = functions.get(name)?;
        match version {
            Some(v) => function_list.iter().find(|f| f.version == v).cloned(),
            None => function_list.last().cloned(),
        }
    }
    pub fn list_functions(&self) -> Vec<String> {
        let functions = self.functions.read().unwrap();
        functions.keys().cloned().collect()
    }
    pub fn get_function_count(&self) -> usize {
        let functions = self.functions.read().unwrap();
        functions.len()
    }
    pub fn get_total_versions(&self) -> usize {
        let functions = self.functions.read().unwrap();
        functions.values().map(|versions| versions.len()).sum()
    }
    pub fn compose_functions(
        &self,
        name: String,
        function_chain: Vec<String>,
    ) -> Result<(), BlockError> {
        let functions = self.functions.read().unwrap();
        DependencyManager::validate_function_chain(&functions, &function_chain)?;
        drop(functions);
        let mut compositions = self.function_compositions.write().unwrap();
        compositions.insert(name, function_chain);
        Ok(())
    }
    pub fn execute_composition(
        &self,
        composition_name: &str,
        initial_args: &[Value],
    ) -> Result<Value, BlockError> {
        let compositions = self.function_compositions.read().unwrap();
        let chain = compositions
            .get(composition_name)
            .ok_or_else(|| BlockError::ProcessingError("Composition not found".into()))?
            .clone();
        drop(compositions);
        let mut current_value = if initial_args.len() == 1 {
            initial_args[0].clone()
        } else {
            Value::Array(initial_args.to_vec())
        };
        for func_name in chain {
            if let Some(func) = self.get_function(&func_name, None) {
                let args = if current_value.is_array() {
                    current_value.as_array().unwrap().clone()
                } else {
                    vec![current_value]
                };
                current_value = func.execute(&args)?;
            } else {
                return Err(BlockError::ProcessingError(format!(
                    "Function {func_name} not found in composition"
                )));
            }
        }
        Ok(current_value)
    }
    pub fn list_compositions(&self) -> Vec<String> {
        let compositions = self.function_compositions.read().unwrap();
        compositions.keys().cloned().collect()
    }
    pub fn process_pending_events(&self) {
        let mut hot_reload_manager = self.hot_reload_manager.write().unwrap();
        let events = hot_reload_manager.get_pending_events();
        drop(hot_reload_manager);
        for event in events {
            if HotReloadManager::should_reload_file(&event) {
                for path in event.paths {
                    if let Some(function_name) =
                        HotReloadManager::extract_function_name_from_path(&path)
                    {
                        if let Ok(new_code) = std::fs::read_to_string(&path) {
                            println!("Reloading function: {function_name} from {path:?}");
                            let _ = self.reload_function_from_file(
                                &function_name,
                                &new_code,
                                Some(path.to_string_lossy().to_string()),
                            );
                        }
                    }
                }
            } else if HotReloadManager::is_new_file(&event) {
                println!("New function file detected: {:?}", event.paths);
            }
        }
    }
    pub fn hot_reload_function(
        &self,
        name: &str,
        new_code: &str,
        source_path: Option<String>,
    ) -> Result<(), BlockError> {
        let mut new_fn = self.compile_function(new_code, "execute")?;
        new_fn.source_path = source_path.clone();
        if let Some(ref path) = source_path {
            let mut hot_reload_manager = self.hot_reload_manager.write().unwrap();
            hot_reload_manager.watch_file(path)?;
        }
        let mut functions = self.functions.write().unwrap();
        if let Some(versions) = functions.get_mut(name) {
            versions.push(new_fn);
        } else {
            functions.insert(name.to_string(), vec![new_fn]);
        }
        Ok(())
    }
    fn reload_function_from_file(
        &self,
        name: &str,
        new_code: &str,
        source_path: Option<String>,
    ) -> Result<(), BlockError> {
        self.hot_reload_function(name, new_code, source_path)
    }
    pub fn get_version_history(
        &self,
        function_name: &str,
    ) -> Option<HashMap<String, DateTime<Utc>>> {
        let version_history = self.version_history.read().unwrap();
        version_history.get(function_name).cloned()
    }
    pub fn rollback_function(&self, name: &str, target_version: &str) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions
            .get_mut(name)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        let target_index = function_list
            .iter()
            .position(|f| f.version == target_version)
            .ok_or_else(|| {
                BlockError::ProcessingError(format!(
                    "Version {target_version} not found for function {name}"
                ))
            })?;
        let target_function = function_list.remove(target_index);
        function_list.push(target_function);
        Ok(())
    }
    pub fn get_performance_metrics(
        &self,
        function_name: &str,
        version: Option<&str>,
    ) -> Option<PerformanceMetrics> {
        let function = self.get_function(function_name, version)?;
        function.get_performance_snapshot()
    }
    pub fn benchmark_function(
        &self,
        name: &str,
        args: &[Value],
        iterations: usize,
    ) -> Result<Duration, BlockError> {
        let function = self
            .get_function(name, None)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = function.execute_safe(args);
        }
        let total_time = start.elapsed();
        Ok(total_time / iterations as u32)
    }
    pub fn get_function_metadata(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Option<HashMap<String, Value>> {
        self.get_function(name, version).map(|f| f.metadata.clone())
    }
    pub fn update_function_metadata(
        &self,
        name: &str,
        version: Option<&str>,
        metadata: HashMap<String, Value>,
    ) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions
            .get_mut(name)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        let function = match version {
            Some(v) => function_list
                .iter_mut()
                .find(|f| f.version == v)
                .ok_or_else(|| BlockError::ProcessingError(format!("Version {v} not found")))?,
            None => function_list
                .last_mut()
                .ok_or_else(|| BlockError::ProcessingError("No versions found".into()))?,
        };
        function.metadata.extend(metadata);
        Ok(())
    }
    pub fn get_function_dependencies(&self, name: &str) -> Option<Vec<String>> {
        self.get_function(name, None)
            .map(|f| f.dependencies.clone())
    }
    pub fn add_function_dependency(
        &self,
        function_name: &str,
        dependency: String,
    ) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions.get_mut(function_name).ok_or_else(|| {
            BlockError::ProcessingError(format!("Function {function_name} not found"))
        })?;
        if let Some(current_function) = function_list.last_mut() {
            current_function.add_dependency(dependency);
        }
        Ok(())
    }
    pub fn remove_function_dependency(
        &self,
        function_name: &str,
        dependency: &str,
    ) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions.get_mut(function_name).ok_or_else(|| {
            BlockError::ProcessingError(format!("Function {function_name} not found"))
        })?;
        if let Some(current_function) = function_list.last_mut() {
            current_function.remove_dependency(dependency);
        }
        Ok(())
    }
    pub fn validate_all_dependencies(&self) -> Result<(), BlockError> {
        let functions = self.functions.read().unwrap();
        DependencyManager::validate_dependencies(&functions)
    }
    pub fn detect_circular_dependencies(&self) -> Result<(), BlockError> {
        let functions = self.functions.read().unwrap();
        DependencyManager::detect_circular_dependencies(&functions)
    }
    pub fn get_dependency_graph(&self) -> HashMap<String, Vec<String>> {
        let functions = self.functions.read().unwrap();
        DependencyManager::get_dependency_graph(&functions)
    }
    pub fn get_functions_by_dependency(&self, dependency: &str) -> Vec<String> {
        let functions = self.functions.read().unwrap();
        DependencyManager::get_functions_by_dependency(&functions, dependency)
    }
    pub fn remove_function(&self, name: &str) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        functions
            .remove(name)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        drop(functions);
        let mut version_history = self.version_history.write().unwrap();
        version_history.remove(name);
        drop(version_history);
        let mut compositions = self.function_compositions.write().unwrap();
        let compositions_to_remove: Vec<String> = compositions
            .iter()
            .filter(|(_, chain)| chain.contains(&name.to_string()))
            .map(|(comp_name, _)| comp_name.clone())
            .collect();
        for comp_name in compositions_to_remove {
            compositions.remove(&comp_name);
        }
        Ok(())
    }
    pub fn clear_all_functions(&self) {
        let mut functions = self.functions.write().unwrap();
        functions.clear();
        drop(functions);
        let mut version_history = self.version_history.write().unwrap();
        version_history.clear();
        drop(version_history);
        let mut compositions = self.function_compositions.write().unwrap();
        compositions.clear();
    }
    pub fn cleanup_old_versions(&self, max_versions_per_function: usize) -> usize {
        let mut cleaned_count = 0;
        let mut functions = self.functions.write().unwrap();
        for function_list in functions.values_mut() {
            if function_list.len() > max_versions_per_function {
                let excess = function_list.len() - max_versions_per_function;
                function_list.drain(0..excess);
                cleaned_count += excess;
            }
        }
        cleaned_count
    }
    pub fn export_function(&self, name: &str, version: Option<&str>) -> Result<Value, BlockError> {
        let function = self
            .get_function(name, version)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        ImportExportManager::export_function(&function, name)
    }
    pub fn import_function(&self, export_data: Value) -> Result<String, BlockError> {
        let import_data = ImportExportManager::import_function_metadata(&export_data)?;
        let mut function = self.compile_function(&import_data.source_code, "execute")?;
        function.version = import_data.version;
        function.created_at = import_data.created_at;
        function.metadata = import_data.metadata;
        function.dependencies = import_data.dependencies;
        function.source_path = import_data.source_path;
        self.register_function(import_data.name.clone(), function);
        Ok(import_data.name)
    }
    pub fn get_executor_stats(&self) -> HashMap<String, Value> {
        let mut stats = HashMap::new();
        stats.insert(
            "total_functions".to_string(),
            Value::Number(self.get_function_count().into()),
        );
        stats.insert(
            "total_versions".to_string(),
            Value::Number(self.get_total_versions().into()),
        );
        let compositions = self.function_compositions.read().unwrap();
        stats.insert(
            "total_compositions".to_string(),
            Value::Number(compositions.len().into()),
        );
        stats
    }
    pub fn get_overall_error_statistics(&self) -> (u64, u64, f64) {
        let functions = self.functions.read().unwrap();
        let mut total_calls = 0;
        let mut total_errors = 0;
        for function_list in functions.values() {
            for function in function_list {
                if let Some(metrics) = function.get_performance_snapshot() {
                    total_calls += metrics.total_calls;
                    total_errors += metrics.error_count;
                }
            }
        }
        let success_rate = if total_calls > 0 {
            (total_calls - total_errors) as f64 / total_calls as f64
        } else {
            1.0
        };
        (total_calls, total_errors, success_rate)
    }
    pub fn get_functions_with_failures(&self) -> Vec<(String, String, u64)> {
        let functions = self.functions.read().unwrap();
        let mut failed_functions = Vec::new();
        for (name, versions) in functions.iter() {
            for function in versions {
                if function.has_failures() {
                    failed_functions.push((
                        name.clone(),
                        function.version.clone(),
                        function.get_error_count(),
                    ));
                }
            }
        }
        failed_functions
    }
    pub fn get_function_performance_report(&self) -> HashMap<String, Value> {
        let functions = self.functions.read().unwrap();
        let mut report = HashMap::new();
        for (name, versions) in functions.iter() {
            let version_reports: Vec<Value> = versions
                .iter()
                .map(|f| {
                    let metrics = f.get_performance_snapshot().unwrap_or_default();
                    serde_json::json!({
                        "version": f.version,
                        "total_calls": metrics.total_calls,
                        "error_count": metrics.error_count,
                        "success_rate": metrics.success_rate,
                        "avg_execution_time_ms": metrics.avg_execution_time.as_millis(),
                        "peak_memory_usage_bytes": metrics.peak_memory_usage,
                    })
                })
                .collect();
            report.insert(name.clone(), Value::Array(version_reports));
        }
        report
    }
    pub fn execute_function(&self, name: &str, args: &[Value]) -> Result<Value, BlockError> {
        let function = self
            .get_function(name, None)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function '{name}' not found")))?;
        function.execute(args)
    }
    pub async fn execute_function_with_timeout(
        &self,
        name: &str,
        args: &[Value],
        timeout: Duration,
    ) -> Result<Value, BlockError> {
        let function = self
            .get_function(name, None)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function '{name}' not found")))?;
        function.execute_with_timeout(args, timeout).await
    }
}
impl Clone for DynamicExecutor {
    fn clone(&self) -> Self {
        Self {
            functions: Arc::clone(&self.functions),
            function_compositions: Arc::clone(&self.function_compositions),
            version_history: Arc::clone(&self.version_history),
            hot_reload_manager: Arc::clone(&self.hot_reload_manager),
            assembly_generator: self.assembly_generator.clone(),
            #[cfg(feature = "dynamic-native")]
            library_registry: Arc::clone(&self.library_registry),
        }
    }
}

#[cfg(feature = "dynamic-wasi")]
impl DynamicExecutor {
    #[allow(dead_code)]
    fn compile_rust_wasi(&self, src: &str) -> Result<DynamicFunction, BlockError> {
        let tmp =
            tempdir().map_err(|e| BlockError::ProcessingError(format!("tempdir failed: {e}")))?;
        let proj = tmp.path().join("wasi_gen");
        fs::create_dir_all(proj.join("src"))
            .map_err(|e| BlockError::ProcessingError(format!("io: {e}")))?;
        fs::write(
            proj.join("Cargo.toml"),
            r#"[package]
name = "wasi_gen"
version = "0.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features=["derive"] }
serde_json = "1"
"#,
        )
        .map_err(|e| BlockError::ProcessingError(format!("write cargo: {e}")))?;

        let adjusted = if src.contains("compute(") {
            src.to_string()
        } else {
            format!(
                r#"use core::{{slice,str}}; use serde_json::{{self, Value}};
#[no_mangle]
pub extern "C" fn compute(inputs_ptr:*const u8,len:usize,out_ptr:*mut u8,out_len:usize)->i32 {{
    if inputs_ptr.is_null() {{ return 1; }}
    let bytes = unsafe {{ slice::from_raw_parts(inputs_ptr,len) }};
    let s = match str::from_utf8(bytes) {{ Ok(v)=>v, Err(_)=> return 2 }};
    let nums: Vec<f64> = match serde_json::from_str::<Vec<f64>>(s) {{ Ok(v)=>v, Err(_)=> return 3 }};
    let result: f64 = {{ {src} }};
    let json = match serde_json::to_string(&result) {{ Ok(v)=>v, Err(_)=> return 4 }};
    let out = unsafe {{ slice::from_raw_parts_mut(out_ptr,out_len) }};
    if json.as_bytes().len()+1 > out.len() {{ return 5; }}
    let bl = json.as_bytes().len();
    out[..bl].copy_from_slice(json.as_bytes());
    out[bl]=0; 0
}}
"#
            )
        };
        fs::write(proj.join("src/lib.rs"), &adjusted)
            .map_err(|e| BlockError::ProcessingError(format!("write lib: {e}")))?;

        let run_build = || -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), BlockError> {
            let out = Command::new("cargo")
                .arg("build")
                .arg("--release")
                .arg("--target")
                .arg("wasm32-wasip1")
                .current_dir(&proj)
                .output()
                .map_err(|e| BlockError::ProcessingError(format!("cargo exec failed: {e}")))?;
            Ok((out.status, out.stdout, out.stderr))
        };
        let (status1, _stdout1, stderr1) = run_build()?;
        let mut final_stderr = stderr1.clone();
        let mut build_succeeded = status1.success();
        if !status1.success() {
            let stderr_txt = String::from_utf8_lossy(&stderr1);
            let has_error_marker = stderr_txt.contains("error:");
            let mut attempted_pow_fix = false;
            if (stderr_txt.contains("E0689") || stderr_txt.contains("ambiguous numeric type"))
                && adjusted.contains(".pow(")
            {
                let fixed = Self::apply_pow_disambiguation(&adjusted);
                if fixed != adjusted {
                    if let Err(e) = fs::write(proj.join("src/lib.rs"), &fixed) {
                        eprintln!("warn: failed to apply wasi pow fix: {e}");
                    } else {
                        attempted_pow_fix = true;
                        let (st_fix, _so_fix, se_fix) = run_build()?;
                        final_stderr = se_fix.clone();
                        if st_fix.success() {
                            build_succeeded = true;
                        }
                    }
                }
            }
            if !build_succeeded {
                if !has_error_marker {
                    let (status2, _stdout2, stderr2) = run_build()?;
                    final_stderr = stderr2.clone();
                    if status2.success() {
                        build_succeeded = true;
                    }
                }
                if !build_succeeded && !attempted_pow_fix {
                    let stderr_snip = String::from_utf8_lossy(&final_stderr);
                    return Err(BlockError::ProcessingError(format!(
                        "wasi build failed: {stderr_snip}"
                    )));
                } else if !build_succeeded {
                    let stderr_snip = String::from_utf8_lossy(&final_stderr);
                    return Err(BlockError::ProcessingError(format!(
                        "wasi build failed (after pow fix attempt): {stderr_snip}"
                    )));
                }
            }
        }

        let wasm_path = proj
            .join("target")
            .join("wasm32-wasip1")
            .join("release")
            .join("wasi_gen.wasm");
        let wasm_bytes = fs::read(&wasm_path)
            .map_err(|e| BlockError::ProcessingError(format!("read wasm: {e}")))?;

        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = wasmtime::Engine::new(&config)
            .map_err(|e| BlockError::ProcessingError(format!("engine: {e}")))?;
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .map_err(|e| BlockError::ProcessingError(format!("compile wasm module: {e}")))?;
        let module_arc = std::sync::Arc::new(module);
        let engine_arc = std::sync::Arc::new(engine);
        let src_owned = adjusted.clone();
        let closure = move |inputs: &[serde_json::Value]| -> Result<serde_json::Value, BlockError> {
            let json = serde_json::to_vec(inputs)
                .map_err(|e| BlockError::ProcessingError(format!("serde: {e}")))?;
            let mut store = wasmtime::Store::new(&engine_arc, ());
            store
                .set_fuel(1_000_000)
                .map_err(|e| BlockError::ProcessingError(format!("fuel: {e}")))?;
            let mut linker = Linker::new(&engine_arc);

            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "fd_write",
                    |_fd: u32, _iovs: u32, _iovs_len: u32, _nwritten: u32| -> u32 { 0 },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub fd_write: {e}")))?;
            linker
                .func_wrap("wasi_snapshot_preview1", "fd_close", |_fd: u32| -> u32 {
                    0
                })
                .map_err(|e| BlockError::ProcessingError(format!("stub fd_close: {e}")))?;
            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "environ_get",
                    |_environ: u32, _environ_buf: u32| -> u32 { 0 },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub environ_get: {e}")))?;
            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "environ_sizes_get",
                    |count_ptr: u32, size_ptr: u32| -> u32 {
                        let _ = (count_ptr, size_ptr);
                        0
                    },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub environ_sizes_get: {e}")))?;
            linker
                .func_wrap("wasi_snapshot_preview1", "proc_exit", |_code: u32| {})
                .map_err(|e| BlockError::ProcessingError(format!("stub proc_exit: {e}")))?;
            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "fd_fdstat_get",
                    |_fd: u32, _buf: u32| -> u32 { 0 },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub fd_fdstat_get: {e}")))?;
            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "random_get",
                    |_ptr: u32, _len: u32| -> u32 { 0 },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub random_get: {e}")))?;
            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "clock_time_get",
                    |_id: u32, _precision: u64, _time_ptr: u32| -> u32 { 0 },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub clock_time_get: {e}")))?;
            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "fd_seek",
                    |_fd: u32, _offset: u64, _whence: u32, _newoffset_ptr: u32| -> u32 { 0 },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub fd_seek: {e}")))?;
            linker
                .func_wrap(
                    "wasi_snapshot_preview1",
                    "fd_read",
                    |_fd: u32, _iovs: u32, _iovs_len: u32, _nread: u32| -> u32 { 0 },
                )
                .map_err(|e| BlockError::ProcessingError(format!("stub fd_read: {e}")))?;
            let instance = linker
                .instantiate(&mut store, &module_arc)
                .map_err(|e| BlockError::ProcessingError(format!("instantiate: {e}")))?;
            let compute = instance
                .get_func(&mut store, "compute")
                .ok_or_else(|| BlockError::ProcessingError("compute export not found".into()))?;
            let typed = compute
                .typed::<(i32, i32, i32, i32), i32>(&store)
                .map_err(|e| BlockError::ProcessingError(format!("typed func: {e}")))?;

            let memory = instance
                .get_memory(&mut store, "memory")
                .ok_or_else(|| BlockError::ProcessingError("memory export not found".into()))?;

            let input_len = json.len() as i32;
            let input_ptr = 64i32;
            let output_ptr = input_ptr + input_len + 16;
            let output_len = 8192i32;

            let required_bytes = (output_ptr + output_len) as usize;
            let required_pages = (required_bytes / 65536) + 1;
            while (memory.size(&store) as usize) < required_pages {
                memory
                    .grow(&mut store, 1)
                    .map_err(|e| BlockError::ProcessingError(format!("memory grow: {e}")))?;
            }
            let data = memory.data_mut(&mut store);
            data[input_ptr as usize..(input_ptr as usize + input_len as usize)]
                .copy_from_slice(&json);
            let code = typed
                .call(&mut store, (input_ptr, input_len, output_ptr, output_len))
                .map_err(|e| BlockError::ProcessingError(format!("call: {e}")))?;
            if code != 0 {
                return Err(BlockError::ProcessingError(format!(
                    "dyn fn error code {code}"
                )));
            }
            let data_after = memory.data(&store);
            let out_slice =
                &data_after[output_ptr as usize..(output_ptr as usize + output_len as usize)];
            let nul = out_slice
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(out_slice.len());
            let json_out = &out_slice[..nul];
            let v: serde_json::Value = serde_json::from_slice(json_out)
                .map_err(|e| BlockError::ProcessingError(format!("json parse: {e}")))?;
            Ok(v)
        };
        Ok(super::function::DynamicFunction::new(
            std::sync::Arc::new(closure),
            format!("v{}", chrono::Utc::now().timestamp()),
            src_owned,
        ))
    }
}
impl Default for DynamicExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default DynamicExecutor")
    }
}
impl Drop for DynamicExecutor {
    fn drop(&mut self) {
        if let Ok(mut _manager) = self.hot_reload_manager.write() {}
    }
}
unsafe impl Send for DynamicExecutor {}
unsafe impl Sync for DynamicExecutor {}
