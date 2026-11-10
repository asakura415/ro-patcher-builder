use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchItem {
    pub index: u32,
    pub file_name: String,
}

/// Parse a plist.txt string into a list of PatchItem.
/// Each valid line contains `<index> <filename>` or `<index>\t<filename>`.
/// Blank lines, leading/trailing whitespace, and comments starting with '#' or '//' are ignored.
pub fn parse_patch_list(content: &str) -> Vec<PatchItem> {
    let mut list = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        if let Some(index_str) = parts.next() {
            if let Ok(index) = index_str.parse::<u32>() {
                if let Some(file_name) = parts.next() {
                    list.push(PatchItem {
                        index,
                        file_name: file_name.to_string(),
                    });
                }
            }
        }
    }

    list.sort_by_key(|p| p.index);
    list
}

/// Serializes a slice of PatchItem back to standard plist.txt string.
pub fn format_patch_list(patches: &[PatchItem]) -> String {
    let mut out = String::new();
    for p in patches {
        out.push_str(&format!("{} {}\n", p.index, p.file_name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plist() {
        let sample = "
        // Comment
        # Another comment
        1 001_initial.thor
        2 002_bgm.thor
        10 010_items.thor
        5 005_map.thor
        ";

        let parsed = parse_patch_list(sample);
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].index, 1);
        assert_eq!(parsed[0].file_name, "001_initial.thor");
        assert_eq!(parsed[1].index, 2);
        assert_eq!(parsed[2].index, 5);
        assert_eq!(parsed[3].index, 10);
    }
}
