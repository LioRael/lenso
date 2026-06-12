use crate::flows::FlowDefinition;
use crate::functions::{FunctionDefinition, FunctionRegistry};
use crate::queues::Queue;
use crate::triggers::TriggerDefinition;

#[derive(Debug, Default, Clone)]
pub struct RuntimeDescriptor {
    pub module: &'static str,
    pub functions: Vec<FunctionDefinition>,
    pub triggers: Vec<TriggerDefinition>,
    pub flows: Vec<FlowDefinition>,
    pub queues: Vec<Queue>,
}

impl RuntimeDescriptor {
    pub fn register_into(&self, registry: &mut FunctionRegistry) {
        for function in self.functions.iter().cloned() {
            registry.register(function);
        }
    }
}
