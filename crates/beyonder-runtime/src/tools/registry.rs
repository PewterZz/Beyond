use std::collections::HashMap;
use std::sync::Arc;
use super::{Tool, shell::ShellExec};

pub struct ToolRegistry {
    tools: HashMap<&'static str, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn all_tools(&self) -> impl Iterator<Item = &Arc<dyn Tool>> {
        self.tools.values()
    }

    pub fn register_builtins(mut self) -> Self {
        self.register(Arc::new(ShellExec));
        self
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new().register_builtins()
    }
}
