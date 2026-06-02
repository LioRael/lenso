#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryDisplaySource {
    ExecutionName(&'static str),
    HttpRequest {
        method: &'static str,
        path: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryDisplayDescriptor {
    pub source: StoryDisplaySource,
    pub display_name: &'static str,
    pub story_title: Option<&'static str>,
}
