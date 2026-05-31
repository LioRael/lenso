#[derive(Debug, Clone)]
pub struct OpenApiFragment {
    pub module: &'static str,
    pub title: &'static str,
}

#[derive(Debug, Default)]
pub struct OpenApiRegistry {
    fragments: Vec<OpenApiFragment>,
}

impl OpenApiRegistry {
    pub fn push(&mut self, fragment: OpenApiFragment) {
        self.fragments.push(fragment);
    }

    pub fn fragments(&self) -> &[OpenApiFragment] {
        &self.fragments
    }
}
