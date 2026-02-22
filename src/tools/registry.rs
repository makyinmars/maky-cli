use std::{collections::HashMap, sync::Arc};

use crate::tools::{ToolHandler, exec::ExecCommandTool, ls::ListFilesTool, read::ReadFileTool};

#[derive(Default)]
pub struct ToolRegistry {
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<H>(&mut self, handler: H)
    where
        H: ToolHandler + 'static,
    {
        self.handlers
            .insert(handler.name().to_string(), Arc::new(handler));
    }

    pub fn resolve(&self, tool_name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.handlers.get(tool_name).cloned()
    }

    pub fn names(&self) -> Vec<&str> {
        self.handlers.keys().map(String::as_str).collect()
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(ListFilesTool);
        registry.register(ReadFileTool);
        registry.register(ExecCommandTool);
        registry
    }
}
