use crate::ports::store::SessionHit;

impl super::TransferState {
    pub(in crate::ui::app) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    pub(super) fn matches(&self) -> Vec<&SessionHit> {
        let query = self.query.text().to_lowercase();
        self.sessions
            .iter()
            .filter(|hit| {
                query.is_empty()
                    || hit
                        .name
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || hit
                        .last_opened_cwd
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query)
                    || hit.excerpt.to_lowercase().contains(&query)
            })
            .collect()
    }
}

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
