#[derive(Debug, Clone)]
pub enum TriggerSource {
    Event(&'static str),
    Schedule(&'static str),
    Manual,
    Webhook(&'static str),
    Signal(&'static str),
}

#[derive(Debug, Clone)]
pub struct TriggerDefinition {
    pub name: &'static str,
    pub source: TriggerSource,
    pub target: &'static str,
}
