#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueName(pub &'static str);

#[derive(Debug, Clone)]
pub struct Queue {
    pub name: QueueName,
    pub concurrency: usize,
}

impl Queue {
    pub fn new(name: &'static str, concurrency: usize) -> Self {
        Self {
            name: QueueName(name),
            concurrency,
        }
    }
}
