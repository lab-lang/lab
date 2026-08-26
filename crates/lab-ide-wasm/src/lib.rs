//! WebAssembly host API for browser editors and embedded desktop surfaces.

use lab_compiler::backend::{
    default_target_profile as compiler_default_target_profile,
    target_capabilities as compiler_target_capabilities,
    validate_target_profile as compiler_validate_target_profile,
};
use lab_ide::Workspace;
use lab_language::{ModuleId, SourceId};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct LabWorkspace {
    workspace: Workspace,
}

#[wasm_bindgen]
impl LabWorkspace {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            workspace: Workspace::new(),
        }
    }

    #[wasm_bindgen(js_name = setDocument)]
    pub fn set_document(&mut self, source: String, version: i64, text: String) {
        self.workspace
            .set_document(SourceId::new(source), version, text);
    }

    /// Register a document under a module name the host already knows,
    /// rather than one guessed from the path. A file in a package takes its
    /// name from that package's manifest, so a host that has read the
    /// manifest supplies the name and keeps paths as the package lays them
    /// out.
    #[wasm_bindgen(js_name = setModuleDocument)]
    pub fn set_module_document(
        &mut self,
        source: String,
        version: i64,
        text: String,
        module: String,
    ) {
        self.workspace.set_module_document(
            SourceId::new(source),
            version,
            text,
            ModuleId::new(module),
        );
    }

    #[wasm_bindgen(js_name = removeDocument)]
    pub fn remove_document(&mut self, source: String) {
        self.workspace.remove_document(&SourceId::new(source));
    }

    pub fn diagnostics(&self, source: String) -> Result<JsValue, JsValue> {
        serialize(self.workspace.diagnostics(&SourceId::new(source)))
    }

    #[wasm_bindgen(js_name = documentSymbols)]
    pub fn document_symbols(&self, source: String) -> Result<JsValue, JsValue> {
        serialize(&self.workspace.document_symbols(&SourceId::new(source)))
    }

    pub fn completions(&self, source: String, offset: usize) -> Result<JsValue, JsValue> {
        serialize(&self.workspace.completions(&SourceId::new(source), offset))
    }

    pub fn hover(&self, source: String, offset: usize) -> Result<JsValue, JsValue> {
        serialize(&self.workspace.hover(&SourceId::new(source), offset))
    }

    pub fn definition(&self, source: String, offset: usize) -> Result<JsValue, JsValue> {
        serialize(&self.workspace.definition(&SourceId::new(source), offset))
    }

    pub fn references(&self, source: String, offset: usize) -> Result<JsValue, JsValue> {
        serialize(&self.workspace.references(&SourceId::new(source), offset))
    }

    pub fn rename(
        &self,
        source: String,
        offset: usize,
        new_name: String,
    ) -> Result<JsValue, JsValue> {
        serialize(
            &self
                .workspace
                .rename(&SourceId::new(source), offset, &new_name),
        )
    }

    #[wasm_bindgen(js_name = semanticTokens)]
    pub fn semantic_tokens(&self, source: String) -> Result<JsValue, JsValue> {
        serialize(&self.workspace.semantic_tokens(&SourceId::new(source)))
    }

    #[wasm_bindgen(js_name = formatDocument)]
    pub fn format_document(&self, source: String) -> Option<String> {
        self.workspace.format_document(&SourceId::new(source))
    }
}

impl Default for LabWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

/// The compiler-owned target catalog used by browser control planes.
#[wasm_bindgen(js_name = targetCapabilities)]
pub fn target_capabilities() -> Result<JsValue, JsValue> {
    serialize(
        &compiler_target_capabilities().map_err(|error| JsValue::from_str(&error.to_string()))?,
    )
}

/// A complete reference profile for a backend, validated by this compiler.
#[wasm_bindgen(js_name = defaultTargetProfile)]
pub fn default_target_profile(backend: String, name: String) -> Result<JsValue, JsValue> {
    serialize(
        &compiler_default_target_profile(&backend, &name)
            .map_err(|error| JsValue::from_str(&error.to_string()))?,
    )
}

/// Parse, semantically validate, canonicalize, and hash target TOML.
#[wasm_bindgen(js_name = validateTargetProfile)]
pub fn validate_target_profile(name: String, contents: String) -> Result<JsValue, JsValue> {
    serialize(
        &compiler_validate_target_profile(&name, &contents)
            .map_err(|error| JsValue::from_str(&error.to_string()))?,
    )
}

fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsValue::from_str(&error.to_string()))
}
