use crate::ports::store::SessionHit;

pub(super) trait SessionHitLabel {
    fn label(&self) -> String;
}

impl SessionHitLabel for SessionHit {
    fn label(&self) -> String {
        let name = self.name.as_deref().unwrap_or_else(|| {
            if self.excerpt.is_empty() {
                "untitled"
            } else {
                &self.excerpt
            }
        });
        format!(
            "{name} · {} thoughts · {}",
            self.thought_count,
            self.last_opened_cwd.display()
        )
    }
}
