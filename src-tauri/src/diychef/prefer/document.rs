use crate::diychef::catalog::Catalog;
use crate::models::*;
use std::collections::HashSet;
#[derive(Clone, Debug)]
pub(crate) enum LineKind {
    Blank,
    Switch,
    Chef {
        key: String,
        hat: String,
        duplicate: bool,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub id: String,
    pub text: String,
    pub ending: String,
    pub kind: LineKind,
}
#[derive(Clone, Debug)]
pub(crate) struct Document {
    pub lines: Vec<Line>,
    pub encoding: String,
    pub newline: String,
}
pub(crate) fn decode(bytes: &[u8]) -> Result<(String, String), String> {
    if bytes.starts_with(&[255, 254, 0, 0]) || bytes.starts_with(&[0, 0, 254, 255]) {
        return Err("diychef.invalidEncoding".into());
    }
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if bytes.len() % 2 != 0 {
            return Err("diychef.invalidEncoding".into());
        }
        let le = bytes[0] == 0xff;
        let words: Vec<_> = bytes[2..]
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        return String::from_utf16(&words)
            .map(|s| (s, if le { "utf16le" } else { "utf16be" }.into()))
            .map_err(|_| "diychef.invalidEncoding".into());
    }
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let data = if bom { &bytes[3..] } else { bytes };
    String::from_utf8(data.to_vec())
        .map(|s| (s, if bom { "utf8bom" } else { "utf8" }.into()))
        .map_err(|_| "diychef.invalidEncoding".into())
}
impl Document {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let (text, encoding) = decode(bytes)?;
        let mut seen = HashSet::new();
        let mut lines = vec![];
        let mut start = 0;
        for (index, c) in text.char_indices() {
            if c != '\n' && c != '\r' {
                continue;
            }
            if index < start {
                continue;
            }
            let end = if c == '\r' && text.as_bytes().get(index + 1) == Some(&b'\n') {
                index + 2
            } else {
                index + 1
            };
            Self::line(
                &text[start..index],
                &text[index..end],
                &mut seen,
                &mut lines,
            );
            start = end;
        }
        if start < text.len() {
            Self::line(&text[start..], "", &mut seen, &mut lines);
        }
        let newline = lines
            .iter()
            .find(|l| !l.ending.is_empty())
            .map(|l| l.ending.clone())
            .unwrap_or_else(|| if cfg!(windows) { "\r\n" } else { "\n" }.into());
        Ok(Self {
            lines,
            encoding,
            newline,
        })
    }

    fn line(text: &str, ending: &str, seen: &mut HashSet<String>, lines: &mut Vec<Line>) {
        let tokens: Vec<_> = text.split([' ', '\t']).filter(|s| !s.is_empty()).collect();
        let kind = if text.contains(super::SWITCH_LINE) {
            LineKind::Switch
        } else if tokens.is_empty() {
            LineKind::Blank
        } else {
            let key = tokens[0].trim().to_string();
            let hat = tokens
                .get(1)
                .map(|s| s.trim())
                .and_then(|s| s.strip_prefix("HAT="))
                .unwrap_or("");
            let hat = match hat {
                "Festive" => "Santa",
                "Baseball" => "Baseballcap",
                x => x,
            }
            .to_string();
            let duplicate = !seen.insert(key.clone());
            LineKind::Chef {
                key,
                hat,
                duplicate,
            }
        };
        lines.push(Line {
            id: format!("row:{}", lines.len()),
            text: text.into(),
            ending: ending.into(),
            kind,
        });
    }
    pub(crate) fn selection(&self, catalog: &Catalog) -> ChefSelection {
        let mut entries = vec![];
        let mut lobby_switch = false;
        for l in &self.lines {
            match &l.kind {
                LineKind::Switch => lobby_switch = true,
                LineKind::Chef {
                    key,
                    hat,
                    duplicate: false,
                } => {
                    let chef_ref = catalog
                        .chefs
                        .iter()
                        .find(|c| c.config_name == *key)
                        .map(|c| ChefReference::Known {
                            chef_id: c.id.clone(),
                        })
                        .unwrap_or_else(|| ChefReference::Unresolved {
                            raw_key: key.clone(),
                            row_id: l.id.clone(),
                        });
                    let hat_ref = catalog
                        .hats
                        .iter()
                        .find(|h| h.config_value == *hat && h.resource_state != "invalid")
                        .map(|h| HatReference::Known {
                            hat_id: h.id.clone(),
                        })
                        .unwrap_or_else(|| HatReference::Unresolved {
                            raw_value: hat.clone(),
                            row_id: l.id.clone(),
                        });
                    entries.push(ChefEntry {
                        entry_id: l.id.clone(),
                        row_id: Some(l.id.clone()),
                        chef_ref,
                        hat_ref,
                    });
                }
                _ => {}
            }
        }
        ChefSelection {
            entries,
            lobby_switch,
        }
    }
    pub(crate) fn rows(&self) -> Vec<ChefDocumentRow> {
        self.lines
            .iter()
            .map(|l| {
                let (category, group) = match &l.kind {
                    LineKind::Blank => ("blank", None),
                    LineKind::Switch => ("switch", None),
                    LineKind::Chef { key, duplicate, .. } => (
                        if *duplicate { "duplicate" } else { "chef" },
                        Some(key.clone()),
                    ),
                };
                ChefDocumentRow {
                    row_id: l.id.clone(),
                    category: category.into(),
                    raw_display: l.text.clone(),
                    duplicate_group_id: group,
                }
            })
            .collect()
    }
    pub(crate) fn encode(&self, text: &str) -> Vec<u8> {
        match self.encoding.as_str() {
            "utf16le" | "utf16be" => {
                let le = self.encoding == "utf16le";
                let mut out = if le { vec![255, 254] } else { vec![254, 255] };
                for w in text.encode_utf16() {
                    out.extend(if le { w.to_le_bytes() } else { w.to_be_bytes() });
                }
                out
            }
            "utf8bom" => {
                let mut out = vec![239, 187, 191];
                out.extend(text.as_bytes());
                out
            }
            _ => text.as_bytes().to_vec(),
        }
    }
    pub(crate) fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| format!("{}{}", l.text, l.ending))
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encodings_and_mixed_endings_roundtrip() {
        for enc in ["utf8", "utf8bom", "utf16le", "utf16be"] {
            let d = Document {
                lines: vec![],
                encoding: enc.into(),
                newline: "\n".into(),
            };
            let b = d.encode("Chef_Black_cat\r\n\t\n未知\r");
            let read = Document::parse(&b).unwrap();
            assert_eq!(read.encode(&read.text()), b);
        }
    }
    #[test]
    fn exact_duplicates_and_switch() {
        let d = Document::parse(b"X HAT=Festive\nX HAT=None\n# LOBBYSWITCHCHEF=TRUE\n").unwrap();
        assert!(matches!(
            d.lines[1].kind,
            LineKind::Chef {
                duplicate: true,
                ..
            }
        ));
        assert!(matches!(d.lines[2].kind, LineKind::Switch));
    }
    #[test]
    fn unicode_trim_applies_to_each_ascii_split_token() {
        let d =
            Document::parse("\u{a0}Chef_Black_cat\u{a0} HAT=None\u{a0} extra".as_bytes()).unwrap();
        assert!(
            matches!(&d.lines[0].kind, LineKind::Chef {key,hat,..} if key == "Chef_Black_cat" && hat == "None")
        );
    }
}
