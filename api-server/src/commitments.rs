use dashmap::DashMap;
use once_cell::sync::Lazy;

const MAX_TAGS: usize = 20;
const MAX_TAG_LENGTH: usize = 64;

static TAGS: Lazy<DashMap<u64, Vec<String>>> = Lazy::new(DashMap::new);

pub fn set_tags(ip_id: u64, tags: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim().to_ascii_lowercase();
        if tag.is_empty() || tag.len() > MAX_TAG_LENGTH {
            return Err(format!("tags must be 1-{MAX_TAG_LENGTH} characters"));
        }
        if !normalized.contains(&tag) {
            normalized.push(tag);
        }
    }
    if normalized.len() > MAX_TAGS {
        return Err(format!("a commitment may have at most {MAX_TAGS} tags"));
    }
    TAGS.insert(ip_id, normalized.clone());
    Ok(normalized)
}

pub fn get_tags(ip_id: u64) -> Vec<String> {
    TAGS.get(&ip_id).map(|tags| tags.clone()).unwrap_or_default()
}

pub fn list_by_tag(tag: &str) -> Vec<u64> {
    let tag = tag.trim().to_ascii_lowercase();
    let mut ids: Vec<u64> = TAGS
        .iter()
        .filter(|entry| entry.value().iter().any(|value| value == &tag))
        .map(|entry| *entry.key())
        .collect();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_deduplicates_tags() {
        assert_eq!(set_tags(1, vec!["Research".into(), "research".into()]).unwrap(), vec!["research"]);
    }

    #[test]
    fn rejects_empty_tags() {
        assert!(set_tags(2, vec![" ".into()]).is_err());
    }
}
